//! install, sync, uninstall, convert, bootstrap. Integrator.
//!
//! Output lines and exit codes follow v0.4.1 `install_commands.py` and
//! `bootstrap_commands.py`. v1 changes: links are the default mode, sync
//! reconciles (connects and prunes, D3), and `convert` is new (spec §6.6).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;
use std::rc::Rc;

use serde::Serialize;
use serde_json::json;

use super::config_error;
use crate::Error;
use crate::check::audit::{
    AuditGateResult, audit_skills, format_gate, gate_json, policy_from_settings,
};
use crate::check::validate::{ValidationResult, compose_validation, validate_skill_for_host};
use crate::cli::*;
use crate::drive::apply::{self, AuditMeta, Connection, CopySnapshot, Outcome, PreparedConnection};
use crate::drive::hosts::{self, get_host};
use crate::drive::link::LinkKind;
use crate::drive::plan::{self, Action, PlanItem, SyncScope};
use crate::drive::state::{State, StateLock, default_state_path};
use crate::platform::paths::display_path;
use crate::registry::manifest::{
    CliOverrides, InstallMode, Manifest, Skill, expand_path, load_manifest, select_names,
    selected_skills,
};
use crate::registry::root::{self, confirm_multi_host_write, resolve_hosts};

// ---------------------------------------------------------------- install

pub fn install(args: &InstallArgs) -> ExitCode {
    if args.json && !args.dry_run {
        return config_error("install --json requires --dry-run");
    }
    let cli = CliOverrides {
        install_mode: args.mode.map(Into::into),
        ..CliOverrides::default()
    };
    let (manifest, resolution, _state_lock) = match load_connect_manifest(&cli, !args.dry_run) {
        Ok(loaded) => loaded,
        Err(e) => return config_error(e),
    };
    let skills = match select_names(&manifest, &args.skills, &args.groups)
        .and_then(|names| selected_skills(&manifest, &names, args.all))
    {
        Ok(skills) => skills,
        Err(e) => return config_error(e),
    };
    let hosts = match resolve_hosts(&manifest, args.host.as_deref()) {
        Ok(hosts) => hosts,
        Err(e) => return config_error(e),
    };
    let allow = is_builtin_seed_default_install(args, &manifest, &skills, &hosts);
    let needs_yes = !allow && hosts.len() > 1 && !args.yes;
    let preview_checks;
    let preview_validation;
    if args.dry_run {
        preview_checks = check_targets(&manifest, &skills, &hosts, None, CheckScope::AllHosts);
        preview_validation = validate_selected(&preview_checks);
        if preview_validation.iter().any(|check| !check.ok) {
            DryRun {
                items: &[],
                validation: &preview_validation,
                audit: None,
                resolution: Some(&resolution),
                needs_yes,
            }
            .print(args.json);
            if !args.json {
                print_confirmation_hint(&hosts, needs_yes);
            }
            return ExitCode::from(1);
        }
    } else {
        preview_checks = Vec::new();
        preview_validation = Vec::new();
    }
    let mut state = match State::load(None) {
        Ok(s) => s,
        Err(e) => return config_error(e),
    };
    let mut items = Vec::new();
    for skill in &skills {
        for host in &hosts {
            let mode = manifest.effective_install_mode(host);
            match plan::plan_install_one(&manifest, skill, host, &state, mode) {
                Ok(item) => items.push(item),
                Err(e) => return config_error(e),
            }
        }
    }

    let prepared = (!args.dry_run).then(|| PreparedPlan::new(&items));
    let checks = if args.dry_run {
        preview_checks
    } else {
        check_targets(
            &manifest,
            &skills,
            &hosts,
            prepared.as_ref(),
            CheckScope::AllHosts,
        )
    };
    let validation = if args.dry_run {
        preview_validation
    } else {
        validate_selected(&checks)
    };
    if !args.dry_run {
        print_validation(&validation, false);
        if validation.iter().any(|check| !check.ok) {
            return ExitCode::from(1);
        }
    }
    let gate = match audit_gate(&manifest, &checks, args.skip_audit) {
        Ok(gate) => gate,
        Err(e) => return config_error(e),
    };

    if args.dry_run {
        DryRun {
            items: &items,
            validation: &validation,
            audit: Some(&gate),
            resolution: Some(&resolution),
            needs_yes,
        }
        .print(args.json);
        if !args.json {
            println!("{}", format_gate(&gate, "AUDIT WOULD BLOCK"));
            println!("Dry run — planned actions (nothing written):");
            print_plan(&items);
            super::print_would_cache_root(&resolution);
            print_confirmation_hint(&hosts, needs_yes);
        }
        return ExitCode::SUCCESS;
    }

    if !allow && !confirm_multi_host_write(&hosts, args.yes) {
        return ExitCode::from(2);
    }
    if let Some(code) = report_gate(&gate) {
        return code;
    }
    let mut audit = audit_meta_by_path(&checks, &gate);
    let mut prepared = prepared.expect("real install has a prepared plan");
    if let Err(code) = check_fallbacks(&manifest, &mut prepared, &mut audit, args.skip_audit) {
        return code;
    }
    if let Err(error) = remember_write_root(Some(&resolution), &items) {
        return config_error(error);
    }
    let (_, blocked) = apply_plan(prepared, &mut state, &audit, false);
    exit(blocked)
}

/// v0.4.1 `is_builtin_seed_default_install`: a plain `install` of only
/// built-in seeds into built-in default paths needs no `--yes`.
fn is_builtin_seed_default_install(
    args: &InstallArgs,
    manifest: &Manifest,
    skills: &[&Skill],
    hosts_: &[String],
) -> bool {
    if !args.skills.is_empty()
        || !args.groups.is_empty()
        || args.host.is_some()
        || args.all
        || args.mode.is_some()
        || args.skip_audit
        || skills.is_empty()
        || skills.iter().any(|s| s.source_type != "builtin-seed")
    {
        return false;
    }
    hosts_.iter().all(|host| {
        hosts::get_host(host).is_some_and(|config| {
            manifest
                .targets
                .get(host)
                .is_some_and(|t| t.path == expand_path(config.default_user_path))
        })
    })
}

// ---------------------------------------------------------------- sync

pub fn sync(args: &SyncArgs) -> ExitCode {
    if args.json && args.check {
        return config_error("sync --json is not supported with --check; use --dry-run --json");
    }
    if args.json && !args.dry_run {
        return config_error("sync --json requires --dry-run");
    }
    let (manifest, resolution, _state_lock) =
        match load_connect_manifest(&CliOverrides::default(), !args.dry_run && !args.check) {
            Ok(loaded) => loaded,
            Err(e) => return config_error(e),
        };
    sync_manifest_with_resolution(&manifest, args, Some(&resolution))
}

/// Sync against an already loaded manifest.
pub fn sync_manifest(manifest: &Manifest, args: &SyncArgs) -> ExitCode {
    let _state_lock = match lock_state(!args.dry_run && !args.check) {
        Ok(lock) => lock,
        Err(e) => return config_error(e),
    };
    sync_manifest_with_resolution(manifest, args, None)
}

fn sync_manifest_with_resolution(
    manifest: &Manifest,
    args: &SyncArgs,
    resolution: Option<&root::RootResolution>,
) -> ExitCode {
    let hosts = match resolve_hosts(manifest, args.host.as_deref()) {
        Ok(h) => h,
        Err(e) => return config_error(e),
    };
    // By default, sync looks at every registered skill so prune sees disabled
    // skills too. `--all` connects disabled skills within the selected scope.
    let requested = match select_names(manifest, &args.skills, &args.groups) {
        Ok(requested) => requested,
        Err(e) => return config_error(e),
    };
    let scoped = match selected_skills(manifest, &requested, true) {
        Ok(skills) => skills,
        Err(e) => return config_error(e),
    };
    let scoped: Vec<Skill> = scoped
        .into_iter()
        .cloned()
        .map(|mut s| {
            if args.all {
                s.enabled = true;
            }
            s
        })
        .collect();
    let skills: Vec<&Skill> = scoped.iter().collect();
    let state = match State::load(None) {
        Ok(s) => s,
        Err(e) => return config_error(e),
    };

    let scope = SyncScope {
        skills: &skills,
        hosts: &hosts,
        include_orphans: requested.is_empty(),
        prune: !args.no_prune,
    };
    let items = match plan::plan_sync(manifest, &state, &scope, None) {
        Ok(items) => items,
        Err(e) => return config_error(e),
    };

    if args.check {
        // Status of every wanted pair (v0.4.1 output), plus what a real sync
        // would prune (D3). Any planned write or block is drift (spec §6.4).
        let mut drift = items
            .iter()
            .any(|i| i.action.writes() || i.action.is_block());
        for skill in &skills {
            if !skill.enabled {
                continue;
            }
            println!("{}", skill.name);
            for host in &hosts {
                match plan::status_of(manifest, skill, host, &state) {
                    Ok(status) => {
                        println!("  {host:8} {}", status.label());
                        drift |= !matches!(status, plan::Status::Fresh | plan::Status::Unsupported);
                    }
                    Err(e) => return config_error(e),
                }
            }
        }
        for item in items.iter().filter(|i| i.action == Action::Remove) {
            println!("{}", item.skill);
            println!("  {:8} PRUNE ({})", item.host, item.reason);
        }
        return exit(drift);
    }

    let writes = items.iter().any(|i| i.action.writes());
    let explicit = !requested.is_empty();
    if !args.dry_run {
        if !writes && !explicit {
            if !args.quiet {
                print_plan_results_noop(&items);
            }
            return exit(items.iter().any(|i| i.action.is_block()));
        }
        if writes && !confirm_multi_host_write(&hosts, args.yes) {
            return ExitCode::from(2);
        }
    }

    let prepared = (!args.dry_run).then(|| PreparedPlan::new(&items));
    let checked: Vec<&Skill> = skills
        .iter()
        .copied()
        .filter(|s| {
            explicit
                || items.iter().any(|i| {
                    i.skill == s.name && matches!(i.action, Action::Create | Action::Update)
                })
        })
        .collect();
    let scope = if explicit {
        CheckScope::Wanted
    } else {
        CheckScope::AllHosts
    };
    let checks = check_targets(manifest, &checked, &hosts, prepared.as_ref(), scope);
    let validation = validate_selected(&checks);
    let invalid = validation.iter().any(|check| !check.ok);
    if !args.dry_run {
        print_validation(&validation, false);
        if invalid {
            return ExitCode::from(1);
        }
    }
    let gate = if (writes || !checks.is_empty()) && !invalid {
        match audit_gate(manifest, &checks, args.skip_audit) {
            Ok(gate) => Some(gate),
            Err(e) => return config_error(e),
        }
    } else {
        None
    };

    if args.dry_run {
        let needs_yes = writes && hosts.len() > 1 && !args.yes;
        DryRun {
            items: &items,
            validation: &validation,
            audit: gate.as_ref(),
            resolution,
            needs_yes,
        }
        .print(args.json);
        if !args.json {
            println!("Dry run — planned actions (nothing written):");
            print_plan(&items);
            // Same rule as the JSON `audit` object: report only a gate that ran.
            if let Some(gate) = gate.as_ref().filter(|gate| gate.skipped || gate.blocked()) {
                println!("{}", format_gate(gate, "AUDIT WOULD BLOCK"));
            }
            if let Some(resolution) = resolution {
                super::print_would_cache_root(resolution);
            }
            print_confirmation_hint(&hosts, needs_yes);
        }
        return exit(invalid);
    }

    if let Some(gate) = gate.as_ref()
        && (!args.quiet || gate.blocked())
        && let Some(code) = report_gate(gate)
    {
        return code;
    }
    let mut state = state;
    let mut audit = gate
        .as_ref()
        .map_or_else(BTreeMap::new, |gate| audit_meta_by_path(&checks, gate));
    let mut prepared = prepared.expect("real sync has a prepared plan");
    if let Err(code) = check_fallbacks(manifest, &mut prepared, &mut audit, args.skip_audit) {
        return code;
    }
    if let Err(error) = remember_write_root(resolution, &items) {
        return config_error(error);
    }
    let (_, blocked) = apply_plan(prepared, &mut state, &audit, args.quiet);
    exit(blocked)
}

fn print_plan_results_noop(items: &[PlanItem]) {
    for item in items {
        print_result_line(item, None);
    }
}

// ---------------------------------------------------------------- uninstall

pub fn uninstall(args: &UninstallArgs) -> ExitCode {
    if args.skills.is_empty() && args.groups.is_empty() {
        return config_error("uninstall requires a skill name");
    }
    let (manifest, resolution, _state_lock) =
        match load_connect_manifest(&CliOverrides::default(), !args.dry_run) {
            Ok(loaded) => loaded,
            Err(e) => return config_error(e),
        };
    let names = match select_names(&manifest, &args.skills, &args.groups) {
        Ok(names) => names,
        // A v1.1 single-name uninstall can remove a managed orphan record.
        Err(_) if args.skills.len() == 1 && args.groups.is_empty() => args.skills.clone(),
        Err(e) => return config_error(e),
    };
    let hosts = match resolve_hosts(&manifest, args.host.as_deref()) {
        Ok(h) => h,
        Err(e) => return config_error(e),
    };
    let mut state = match State::load(None) {
        Ok(s) => s,
        Err(e) => return config_error(e),
    };
    let mut items = Vec::new();
    for name in &names {
        for host in &hosts {
            match plan::plan_uninstall_one(&manifest, name, host, &state) {
                Ok(item) => items.push(item),
                Err(e) => return config_error(e),
            }
        }
    }
    if args.dry_run {
        print_write_refusal(resolution.write_refusal().as_deref());
        println!("Dry run — planned actions (nothing written):");
        print_plan(&items);
        super::print_would_cache_root(&resolution);
        print_confirmation_hint(&hosts, hosts.len() > 1 && !args.yes);
        return ExitCode::SUCCESS;
    }
    if !confirm_multi_host_write(&hosts, args.yes) {
        return ExitCode::from(2);
    }
    if let Err(error) = remember_write_root(Some(&resolution), &items) {
        return config_error(error);
    }
    let mut warned = false;
    let mut failed = false;
    for item in &items {
        match item.action {
            Action::Remove => match apply::apply_prepared(item, &mut state, None, None) {
                Ok(applied) => {
                    println!("  removed: {} -> {}", item.skill, item.host);
                    applied.print_cleanup_warnings();
                }
                Err(Error::StateSave(e)) => return config_error(e),
                Err(e) => {
                    failed = true;
                    println!("  FAILED: {} -> {} ({e})", item.skill, item.host);
                }
            },
            Action::BlockDrift => {
                warned = true;
                println!(
                    "  WARN: {} -> {} was modified locally; not removed",
                    item.skill, item.host
                );
                println!("    {}", display_path(&item.destination));
            }
            action if action.is_block() => {
                warned = true;
                print_result_line(item, None);
            }
            Action::NotManaged => println!(
                "  skipped: {} -> {} (not managed by my-skills)",
                item.skill, item.host
            ),
            _ => {}
        }
    }
    exit(warned || failed)
}

// ---------------------------------------------------------------- convert

pub fn convert(args: &ConvertArgs) -> ExitCode {
    let (manifest, resolution, _state_lock) =
        match load_connect_manifest(&CliOverrides::default(), !args.dry_run) {
            Ok(loaded) => loaded,
            Err(e) => return config_error(e),
        };
    // Convert acts on existing connections, so every registered skill is in
    // scope unless one is named.
    let (skills, hosts) = match root::select_requested(
        &manifest,
        args.skill.as_deref(),
        true,
        args.host.as_deref(),
    ) {
        Ok(selection) => selection,
        Err(e) => return config_error(e),
    };
    let mut state = match State::load(None) {
        Ok(s) => s,
        Err(e) => return config_error(e),
    };
    let to: InstallMode = args.to.into();
    let explicit_host = matches!(args.host.as_deref(), Some(name) if name != "all");
    let (active_hosts, skipped_hosts) =
        plan::filter_convert_hosts(&manifest, &hosts, to, explicit_host);
    let items = match plan::plan_convert(&manifest, &state, &skills, &active_hosts, to) {
        Ok(items) => items,
        Err(e) => return config_error(e),
    };
    if args.dry_run {
        print_write_refusal(resolution.write_refusal().as_deref());
        println!("Dry run — planned actions (nothing written):");
        print_plan(&items);
        print_skipped_convert_hosts(&skipped_hosts);
        super::print_would_cache_root(&resolution);
        print_confirmation_hint(&active_hosts, active_hosts.len() > 1 && !args.yes);
        return ExitCode::SUCCESS;
    }
    if !confirm_multi_host_write(&active_hosts, args.yes) {
        return ExitCode::from(2);
    }
    print_skipped_convert_hosts(&skipped_hosts);
    let prepared = PreparedPlan::new(&items);
    let checks = check_targets(
        &manifest,
        &skills,
        &active_hosts,
        Some(&prepared),
        CheckScope::ConvertCopies,
    );
    let validation = validate_selected(&checks);
    print_validation(&validation, false);
    if validation.iter().any(|check| !check.ok) {
        return ExitCode::from(1);
    }
    let audit = if checks.is_empty() {
        BTreeMap::new()
    } else {
        let gate = match audit_gate(&manifest, &checks, false) {
            Ok(gate) => gate,
            Err(e) => return config_error(e),
        };
        if let Some(code) = report_gate(&gate) {
            return code;
        }
        audit_meta_by_path(&checks, &gate)
    };
    if let Err(error) = remember_write_root(Some(&resolution), &items) {
        return config_error(error);
    }
    let (_, blocked) = apply_plan(prepared, &mut state, &audit, false);
    exit(blocked)
}

fn print_skipped_convert_hosts(skipped: &[(String, InstallMode)]) {
    for (host, mode) in skipped {
        println!(
            "  skipped host {host} ([targets.{host}] install_mode = \"{}\")",
            mode_label(*mode)
        );
    }
}

// ---------------------------------------------------------------- bootstrap

pub fn bootstrap(args: &BootstrapArgs) -> ExitCode {
    let writes = !args.dry_run && !args.skip_skill_install;
    let resolution = match if writes {
        resolve_write_root()
    } else {
        root::resolve_root(None, false)
    } {
        Ok(resolution) => resolution,
        Err(e) => return config_error(e),
    };
    let mut preview = vec!["my-skills".to_string(), "sync".into()];
    if let Some(host) = &args.host {
        preview.extend(["--host".into(), host.clone()]);
    }
    if args.all {
        preview.push("--all".into());
    }
    if args.dry_run {
        println!("Dry run - bootstrap actions (nothing installed):");
        if !args.skip_skill_install {
            preview.extend(["--yes".into(), "--dry-run".into()]);
            println!("  would run: {}", preview.join(" "));
            if let Some(mode) = args.mode {
                let mode: InstallMode = mode.into();
                println!(
                    "  with install mode: {} (from bootstrap --mode)",
                    mode_label(mode)
                );
            }
        }
        println!("  would suggest: run `my-skills hooks install` to check commits");
        return ExitCode::SUCCESS;
    }
    if !args.skip_skill_install {
        println!("Connecting enabled skills into agent hosts...");
        let cli = CliOverrides {
            install_mode: args.mode.map(Into::into),
            ..CliOverrides::default()
        };
        let _state_lock = match StateLock::acquire(&default_state_path()) {
            Ok(lock) => lock,
            Err(e) => return config_error(e),
        };
        let manifest = match load_manifest(&resolution.root, &cli) {
            Ok(m) => m,
            Err(e) => return config_error(e),
        };
        let sync_args = SyncArgs {
            host: args.host.clone(),
            all: args.all,
            yes: true,
            ..SyncArgs::default()
        };
        let code = sync_manifest_with_resolution(&manifest, &sync_args, Some(&resolution));
        if code != ExitCode::SUCCESS {
            return code;
        }
    }
    println!(
        "Bootstrap complete. Run `my-skills hooks install` to check commits, \
         and `my-skills doctor` to inspect registry configuration."
    );
    ExitCode::SUCCESS
}

// ---------------------------------------------------------------- shared

fn load_connect_manifest(
    cli: &CliOverrides,
    writes: bool,
) -> crate::Result<(Manifest, root::RootResolution, Option<StateLock>)> {
    if !writes {
        let (manifest, resolution) = root::load_manifest_resolved(cli, false)?;
        return Ok((manifest, resolution, None));
    }
    let resolution = resolve_write_root()?;
    let state_lock = lock_state(true)?;
    let manifest = load_manifest(&resolution.root, cli)?;
    Ok((manifest, resolution, state_lock))
}

fn lock_state(writes: bool) -> crate::Result<Option<StateLock>> {
    writes
        .then(|| StateLock::acquire(&default_state_path()))
        .transpose()
}

fn resolve_write_root() -> crate::Result<root::RootResolution> {
    let resolution = root::resolve_root(None, false)?;
    if let Some(message) = resolution.write_refusal() {
        return Err(Error::Manifest(message));
    }
    Ok(resolution)
}

// After all checks pass, remember the registry only for a plan that will write.
fn remember_write_root(
    resolution: Option<&root::RootResolution>,
    items: &[PlanItem],
) -> crate::Result<()> {
    if let Some(resolution) = resolution.filter(|resolution| resolution.would_cache())
        && items.iter().any(|item| item.action.writes())
    {
        root::resolve_root(Some(&resolution.root), true)?;
    }
    Ok(())
}

#[derive(Serialize)]
struct ValidationCheck<'a> {
    skill: &'a str,
    host: Option<&'static str>,
    ok: bool,
    errors: Vec<String>,
    warnings: Vec<String>,
}

impl<'a> ValidationCheck<'a> {
    fn new(skill: &'a str, host: Option<&'static str>, result: ValidationResult) -> Self {
        Self {
            skill,
            host,
            ok: result.ok(),
            errors: result.errors,
            warnings: result.warnings,
        }
    }
}

type Preparation = crate::Result<Option<PreparedConnection>>;

struct PreparedPlan<'a> {
    items: &'a [PlanItem],
    connections: BTreeMap<(&'a str, &'a str), Preparation>,
    snapshots: BTreeMap<String, Rc<CopySnapshot>>,
}

impl<'a> PreparedPlan<'a> {
    fn new(items: &'a [PlanItem]) -> Self {
        let mut snapshots = BTreeMap::new();
        let connections = items
            .iter()
            .filter(|item| item.action.writes())
            .map(|item| {
                let connection = if creates_link(item) {
                    Ok(None)
                } else {
                    apply::prepare(item, &mut snapshots)
                };
                ((item.skill.as_str(), item.host.as_str()), connection)
            })
            .collect();
        Self {
            items,
            connections,
            snapshots,
        }
    }

    fn prepare_links(&mut self) -> Vec<CheckTarget<'a>> {
        let mut fallbacks = BTreeMap::new();
        // Check the live canonical targets before creating even temporary
        // links. The old gate covers canonical, then any fallback has its
        // own checks on the bytes that will be installed.
        for item in self.items.iter().filter(|item| creates_link(item)) {
            let prepared = apply::prepare(item, &mut self.snapshots);
            let skill = item.skill.as_str();
            let host = item.host.as_str();
            if let Ok(Some(connection)) = &prepared
                && let Some(path) = connection.check_snapshot()
            {
                fallbacks
                    .entry(skill)
                    .or_insert_with(|| CheckTarget {
                        skill,
                        dir: path.to_owned(),
                        hosts: Vec::new(),
                    })
                    .hosts
                    .push(host);
            }
            self.connections.insert((skill, host), prepared);
        }
        fallbacks.into_values().collect()
    }

    fn snapshot<'b>(&'b self, skill: &'b str, host: &'b str) -> Option<&'b std::path::Path> {
        self.connections
            .get(&(skill, host))?
            .as_ref()
            .ok()?
            .as_ref()?
            .check_snapshot()
    }
}

fn creates_link(item: &PlanItem) -> bool {
    item.action == Action::Create && item.mode == InstallMode::Link
}

#[derive(Clone, Copy)]
enum CheckScope {
    AllHosts,
    Wanted,
    ConvertCopies,
}

struct CheckTarget<'a> {
    skill: &'a str,
    dir: PathBuf,
    hosts: Vec<&'a str>,
}

fn check_targets<'a>(
    manifest: &Manifest,
    skills: &[&'a Skill],
    hosts: &'a [String],
    prepared: Option<&PreparedPlan<'_>>,
    scope: CheckScope,
) -> Vec<CheckTarget<'a>> {
    let mut checks = Vec::new();
    for skill in skills {
        let mut canonical_hosts = Vec::new();
        let mut copy_hosts = Vec::new();
        let mut snapshot = None;
        for host in hosts {
            let eligible = match scope {
                CheckScope::AllHosts => skill.hosts.is_empty() || skill.hosts.contains(host),
                CheckScope::Wanted => plan::wants(manifest, skill, host),
                CheckScope::ConvertCopies => prepared.is_some_and(|plan| {
                    plan.items.iter().any(|item| {
                        item.skill == skill.name
                            && item.host == *host
                            && item.action == Action::Convert
                            && item.mode == InstallMode::Copy
                    })
                }),
            };
            if !eligible {
                continue;
            }
            if let Some(path) = prepared.and_then(|plan| plan.snapshot(&skill.name, host)) {
                snapshot = Some(path);
                copy_hosts.push(host.as_str());
            } else {
                canonical_hosts.push(host.as_str());
            }
        }
        if !canonical_hosts.is_empty()
            || (snapshot.is_none() && matches!(scope, CheckScope::AllHosts))
        {
            checks.push(CheckTarget {
                skill: &skill.name,
                dir: manifest.skill_dir(&skill.name),
                hosts: canonical_hosts,
            });
        }
        if let Some(path) = snapshot {
            checks.push(CheckTarget {
                skill: &skill.name,
                dir: path.to_owned(),
                hosts: copy_hosts,
            });
        }
    }
    checks
}

fn validate_selected<'a>(targets: &[CheckTarget<'a>]) -> Vec<ValidationCheck<'a>> {
    let mut checks = Vec::new();
    for target in targets {
        checks.push(ValidationCheck::new(
            target.skill,
            None,
            compose_validation(&target.dir),
        ));
        for host in target.hosts.iter().filter_map(|host| get_host(host)) {
            checks.push(ValidationCheck::new(
                target.skill,
                Some(host.name),
                validate_skill_for_host(&target.dir, host),
            ));
        }
    }
    checks
}

fn print_validation(checks: &[ValidationCheck<'_>], json: bool) {
    let print = |line: &str| {
        if json {
            eprintln!("{line}");
        } else {
            println!("{line}");
        }
    };
    let mut invalid = false;
    for check in checks {
        if !check.ok {
            invalid = true;
            let host = check.host.map_or(String::new(), |host| format!("{host} "));
            print(&format!(
                "[BLOCKED] {}: {host}validation failed",
                check.skill
            ));
            for error in &check.errors {
                print(&format!("  error: {error}"));
            }
        }
        // With --json the warnings are already in `validation[].warnings`.
        if check.host.is_some() && !json {
            for warning in &check.warnings {
                print(&format!("  warn: {warning}"));
            }
        }
    }
    if invalid {
        eprintln!("\nNo files were changed (fix validation errors first).");
    }
}

fn audit_gate(
    manifest: &Manifest,
    checks: &[CheckTarget<'_>],
    skip: bool,
) -> crate::Result<AuditGateResult> {
    let policy = policy_from_settings(&manifest.audit)?;
    let dirs: Vec<PathBuf> = checks.iter().map(|target| target.dir.clone()).collect();
    audit_skills(&dirs, &policy, skip)
}

fn check_fallbacks(
    manifest: &Manifest,
    prepared: &mut PreparedPlan<'_>,
    audit: &mut BTreeMap<PathBuf, AuditMeta>,
    skip_audit: bool,
) -> Result<(), ExitCode> {
    let checks = prepared.prepare_links();
    if checks.is_empty() {
        return Ok(());
    }
    let validation = validate_selected(&checks);
    print_validation(&validation, false);
    if validation.iter().any(|check| !check.ok) {
        return Err(ExitCode::from(1));
    }
    let gate = audit_gate(manifest, &checks, skip_audit).map_err(config_error)?;
    if gate.blocked()
        && let Some(code) = report_gate(&gate)
    {
        return Err(code);
    }
    audit.extend(audit_meta_by_path(&checks, &gate));
    Ok(())
}

/// Print a skipped or blocked gate; `Some(exit)` when the command must stop.
fn report_gate(gate: &AuditGateResult) -> Option<ExitCode> {
    if gate.skipped {
        println!("{}", format_gate(gate, "AUDIT BLOCKED"));
    } else if gate.blocked() {
        println!("{}", format_gate(gate, "AUDIT BLOCKED"));
        eprintln!("\nNo files were changed (audit blocked).");
        return Some(ExitCode::from(1));
    }
    None
}

fn audit_meta_by_path(
    checks: &[CheckTarget<'_>],
    gate: &AuditGateResult,
) -> BTreeMap<PathBuf, AuditMeta> {
    let now = apply::utc_now();
    gate.results
        .iter()
        .zip(checks)
        .map(|(r, check)| {
            (
                check.dir.clone(),
                AuditMeta {
                    last_audit_at: now.clone(),
                    last_audit_result_hash: r.result_hash(),
                    audit_profile: r.profile.clone(),
                    audit_threshold: r.threshold.map_or("none", |t| t.label()).to_string(),
                },
            )
        })
        .collect()
}

/// Apply writes in order and print v0.4.1-style result lines.
/// Returns (changed, blocked).
fn apply_plan(
    mut plan: PreparedPlan<'_>,
    state: &mut State,
    audit: &BTreeMap<PathBuf, AuditMeta>,
    quiet: bool,
) -> (usize, bool) {
    let mut changed = 0;
    let mut blocked = false;
    for item in plan.items {
        if item.action.writes() {
            let prepared = plan
                .connections
                .remove(&(item.skill.as_str(), item.host.as_str()))
                .expect("every write has a preparation result");
            match prepared.and_then(|connection| {
                let path = connection
                    .as_ref()
                    .and_then(PreparedConnection::check_snapshot)
                    .unwrap_or(&item.source);
                apply::apply_prepared(item, state, audit.get(path), connection.as_ref())
            }) {
                Ok(applied) => {
                    changed += 1;
                    print_result_line(item, Some(&applied.outcome));
                    applied.print_cleanup_warnings();
                }
                Err(Error::StateSave(e)) => {
                    blocked = true;
                    println!("  FAILED: {} -> {} (state: {e})", item.skill, item.host);
                }
                Err(e) => {
                    blocked = true;
                    println!("  FAILED: {} -> {} ({e})", item.skill, item.host);
                }
            }
        } else if item.action.is_block() {
            blocked = true;
            print_result_line(item, None);
        } else if !quiet {
            print_result_line(item, None);
        }
    }
    (changed, blocked)
}

fn connection_suffix(connection: &Connection) -> String {
    match connection {
        Connection::Link(kind) => format!(" (link: {})", link_kind_label(*kind)),
        Connection::Copy => String::new(),
        Connection::CopyFallback { reason } => format!(" [copy fallback: {reason}]"),
    }
}

fn link_kind_label(kind: LinkKind) -> &'static str {
    match kind {
        LinkKind::Symlink => "symlink",
        LinkKind::Junction => "junction",
    }
}

fn print_result_line(item: &PlanItem, outcome: Option<&Outcome>) {
    let pair = format!("{} -> {}", item.skill, item.host);
    match (item.action, outcome) {
        (_, Some(Outcome::Created(c))) => println!("  created: {pair}{}", connection_suffix(c)),
        (_, Some(Outcome::Updated)) => println!("  updated: {pair}"),
        (_, Some(Outcome::Removed)) => println!("  removed: {pair} ({})", item.reason),
        (_, Some(Outcome::Converted(c))) => {
            println!("  converted: {pair}{}", connection_suffix(c))
        }
        (Action::Noop, _) => println!("  unchanged: {pair}"),
        (Action::SkipUnsupported, _) => println!("  skipped: {pair} ({})", item.reason),
        (action, _) if action.is_block() => {
            println!("  BLOCKED: {pair}");
            println!("    {}:", item.reason);
            println!("    {}", display_path(&item.destination));
        }
        _ => {}
    }
}

fn mode_label(mode: InstallMode) -> &'static str {
    match mode {
        InstallMode::Link => "link",
        InstallMode::Copy => "copy",
    }
}

fn print_plan(items: &[PlanItem]) {
    for item in items {
        let tag = if item.mode == InstallMode::Link {
            " [link]"
        } else {
            ""
        };
        println!(
            "  {:16} {} -> {}{tag}  ({})",
            item.action.label(),
            item.skill,
            item.host,
            item.reason
        );
    }
}

fn print_write_refusal(message: Option<&str>) {
    if let Some(message) = message {
        println!("BLOCKED: {message}");
    }
}

fn print_confirmation_hint(hosts: &[String], needs_yes: bool) {
    if needs_yes {
        println!(
            "Re-run with --yes to apply: this command writes to multiple hosts ({}).",
            hosts.join(", ")
        );
    }
}

struct DryRun<'a> {
    items: &'a [PlanItem],
    validation: &'a [ValidationCheck<'a>],
    audit: Option<&'a AuditGateResult>,
    resolution: Option<&'a root::RootResolution>,
    needs_yes: bool,
}

impl DryRun<'_> {
    fn print(&self, json: bool) {
        let refusal = self
            .resolution
            .and_then(root::RootResolution::write_refusal);
        let validation = self.validation.iter().filter(|check| !check.ok);
        let audit = self
            .audit
            .into_iter()
            .flat_map(|gate| &gate.results)
            .filter(|result| result.blocked());
        let plan = self.items.iter().filter(|item| item.action.is_block());
        let (validation_count, audit_count, plan_count) = (
            validation.clone().count(),
            audit.clone().count(),
            plan.clone().count(),
        );
        let count = validation_count + audit_count + plan_count;
        let blocked = count != 0 || refusal.is_some();

        if json {
            let mut reasons = Vec::new();
            if let Some(message) = refusal {
                reasons.push(json!({
                    "skill": null,
                    "host": null,
                    "kind": "inactive_registry",
                    "message": message,
                }));
            }
            reasons.extend(validation.map(|check| {
                json!({
                    "skill": check.skill,
                    "host": check.host,
                    "kind": "validation",
                    "message": check.errors.join("\n"),
                })
            }));
            reasons.extend(audit.map(|result| json!({
                "skill": result.skill,
                "host": null,
                "kind": "audit",
                "message": format!("audit blocked (threshold={})", result.threshold.map_or("none", |threshold| threshold.label())),
            })));
            reasons.extend(plan.map(|item| {
                json!({
                    "skill": item.skill,
                    "host": item.host,
                    "kind": item.action.label().to_ascii_lowercase(),
                    "message": item.reason,
                })
            }));
            if self.needs_yes {
                reasons.push(json!({
                    "skill": null,
                    "host": null,
                    "kind": "needs_yes",
                    "message": "this command writes to multiple hosts and requires --yes",
                }));
            }
            let audit_not_run = AuditGateResult {
                results: Vec::new(),
                skipped: false,
            };
            let payload = json!({
                "actions": self.items.iter().map(|item| json!({
                    "skill": item.skill,
                    "host": item.host,
                    "action": item.action.label(),
                    "reason": item.reason,
                    "mode": mode_label(item.mode),
                    "source": display_path(&item.source),
                    "destination": display_path(&item.destination),
                })).collect::<Vec<_>>(),
                "audit": gate_json(self.audit.unwrap_or(&audit_not_run)),
                "registry": self.resolution.map(root::RootResolution::meta),
                "dry_run": true,
                "blocked": blocked,
                "applicable": !blocked && !self.needs_yes,
                "block_reasons": reasons,
                "validation": self.validation,
            });
            println!(
                "{}",
                serde_json::to_string_pretty(&payload).expect("JSON values always serialize")
            );
        } else {
            print_write_refusal(refusal.as_deref());
            if count != 0 {
                println!(
                    "BLOCKED: {count} item(s) would be refused (validation {validation_count}, audit {audit_count}, plan {plan_count})"
                );
            }
        }
        print_validation(self.validation, json);
    }
}

fn exit(blocked: bool) -> ExitCode {
    if blocked {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::drive::link::{self, EntryKind};

    #[test]
    fn conversion_persists_state_without_a_caller_save() {
        let temp = tempfile::tempdir().unwrap();
        let state_path = temp.path().join("state.json");
        let mut state = State::load(Some(&state_path)).unwrap();
        let source = temp.path().join("registry").join("alpha");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("SKILL.md"), "canonical").unwrap();

        let mut item = PlanItem {
            skill: "alpha".into(),
            host: "claude".into(),
            source,
            destination: temp.path().join("host").join("alpha"),
            action: Action::Create,
            reason: String::new(),
            mode: InstallMode::Copy,
            source_hash: None,
        };
        let audit = BTreeMap::new();
        let items = std::slice::from_ref(&item);
        assert_eq!(
            apply_plan(PreparedPlan::new(items), &mut state, &audit, false),
            (1, false)
        );
        assert_eq!(State::load(Some(&state_path)).unwrap(), state);

        item.action = Action::Convert;
        for mode in [InstallMode::Link, InstallMode::Copy] {
            item.mode = mode;
            let items = std::slice::from_ref(&item);
            assert_eq!(
                apply_plan(PreparedPlan::new(items), &mut state, &audit, false),
                (1, false)
            );
            let persisted = State::load(Some(&state_path)).unwrap();
            assert_eq!(persisted, state);
            assert_eq!(persisted.get("alpha", "claude").unwrap().mode, mode);
            assert_eq!(
                matches!(
                    link::entry_kind(&item.destination).unwrap(),
                    EntryKind::Link(_)
                ),
                mode == InstallMode::Link
            );
        }
    }
}
