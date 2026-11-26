//! install, sync, uninstall, convert, bootstrap. Integrator.
//!
//! Output lines and exit codes follow v0.4.1 `install_commands.py` and
//! `bootstrap_commands.py`. v1 changes: links are the default mode, sync
//! reconciles (connects and prunes, D3), and `convert` is new (spec §6.6).

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::process::ExitCode;
use std::rc::Rc;

use serde::Serialize;
use serde_json::{Value, json};

use super::config_error;
use crate::Error;
use crate::check::validate::{ValidationResult, validate_skill, validate_skill_for_host};
use crate::cli::*;
use crate::drive::apply::{self, Connection, CopySnapshot, Outcome, PreparedConnection};
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
    let cli = CliOverrides {
        install_mode: args.mode.map(Into::into),
        ..CliOverrides::default()
    };
    let ConnectManifest {
        manifest,
        resolution,
        _state_lock,
        refusal,
    } = match load_connect_manifest(&cli, !args.dry_run, args.json) {
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

    let shared = match plan::group_destinations(&manifest, &state, &mut items) {
        Ok(shared) => shared,
        Err(err) => return config_error(err),
    };
    let mut report = PlanReport::unchecked(&items, Some(&resolution), needs_yes);
    if let Some(refusal) = &refusal {
        return report.stop(BatchStop::Inactive(refusal), args.json);
    }
    if shared.incompatible && !args.dry_run {
        if !args.json {
            print_plan_results_noop(&items);
        }
        return report.stop(BatchStop::Blocked, args.json);
    }

    let prepared = PreparedPlan::new(&items).with_groups(shared.groups);
    let checks = check_targets(&manifest, &skills, &hosts, &prepared, CheckScope::AllHosts);
    let validation = validate_selected(&checks, Some(&prepared));
    report.validation = &validation;
    if !args.dry_run {
        print_validation(&validation, args.json);
    }
    if validation.iter().any(|check| !check.ok) {
        if !args.dry_run {
            return report.stop(BatchStop::Blocked, args.json);
        }
        report.items = &[];
        report.print_preview(args.json);
        if !args.json {
            print_confirmation_hint(&hosts, needs_yes);
        }
        return ExitCode::from(1);
    }

    if args.dry_run {
        report.print_preview(args.json);
        if !args.json {
            println!("Dry run — planned actions (nothing written):");
            print_plan(&items);
            super::print_would_cache_root(&resolution);
            print_confirmation_hint(&hosts, needs_yes);
        }
        return ExitCode::SUCCESS;
    }

    if !allow && !confirm_multi_host_write(&hosts, args.yes) {
        return report.stop(BatchStop::NeedsYes, args.json);
    }
    apply_connections(
        prepared,
        &mut state,
        report,
        ApplyOptions {
            json: args.json,
            quiet: false,
        },
    )
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
    let ConnectManifest {
        manifest,
        resolution,
        _state_lock,
        refusal,
    } = match load_connect_manifest(
        &CliOverrides::default(),
        !args.dry_run && !args.check,
        args.json,
    ) {
        Ok(loaded) => loaded,
        Err(e) => return config_error(e),
    };
    sync_manifest_with_resolution(&manifest, args, Some(&resolution), refusal.as_deref())
}

/// Sync against an already loaded manifest.
pub fn sync_manifest(manifest: &Manifest, args: &SyncArgs) -> ExitCode {
    if args.json && args.check {
        return config_error("sync --json is not supported with --check; use --dry-run --json");
    }
    let _state_lock = match lock_state(!args.dry_run && !args.check) {
        Ok(lock) => lock,
        Err(e) => return config_error(e),
    };
    sync_manifest_with_resolution(manifest, args, None, None)
}

/// Sync for a caller that already holds the state lock (the post-commit hook).
pub fn sync_locked(
    manifest: &Manifest,
    args: &SyncArgs,
    resolution: &root::RootResolution,
) -> ExitCode {
    sync_manifest_with_resolution(manifest, args, Some(resolution), None)
}

fn sync_manifest_with_resolution(
    manifest: &Manifest,
    args: &SyncArgs,
    resolution: Option<&root::RootResolution>,
    refusal: Option<&str>,
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
        .filter(|skill| !args.exclude.contains(&skill.name))
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
    let mut items = match plan::plan_sync(manifest, &state, &scope, None) {
        Ok(items) => items,
        Err(e) => return config_error(e),
    };
    // Excluded skills are untouched, including by the orphan prune.
    items.retain(|item| !args.exclude.contains(&item.skill));

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

    let shared = match plan::group_destinations(manifest, &state, &mut items) {
        Ok(shared) => shared,
        Err(err) => return config_error(err),
    };
    let writes = items.iter().any(|i| i.action.writes());
    let explicit = !requested.is_empty();
    let mut report =
        PlanReport::unchecked(&items, resolution, writes && hosts.len() > 1 && !args.yes);
    if let Some(refusal) = refusal {
        return report.stop(BatchStop::Inactive(refusal), args.json);
    }
    if shared.incompatible && !args.dry_run {
        if !args.json {
            print_plan_results_noop(&items);
        }
        return report.stop(BatchStop::Blocked, args.json);
    }
    let mut state = state;
    if !args.dry_run {
        if !writes && !explicit {
            if args.json {
                let batch = apply_plan(PreparedPlan::new(&items), &mut state, args.quiet, true);
                report.print_applied(&batch, None);
            } else if !args.quiet {
                print_plan_results_noop(&items);
            }
            return exit(items.iter().any(|i| i.action.is_block()));
        }
        if writes && !confirm_multi_host_write(&hosts, args.yes) {
            return report.stop(BatchStop::NeedsYes, args.json);
        }
    }

    let prepared = PreparedPlan::new(&items).with_groups(shared.groups);
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
    let checks = check_targets(manifest, &checked, &hosts, &prepared, scope);
    let validation = validate_selected(&checks, Some(&prepared));
    report.validation = &validation;
    let invalid = validation.iter().any(|check| !check.ok);
    if !args.dry_run {
        print_validation(&validation, args.json);
        if invalid {
            return report.stop(BatchStop::Blocked, args.json);
        }
    }
    if args.dry_run {
        report.print_preview(args.json);
        if !args.json {
            println!("Dry run — planned actions (nothing written):");
            print_plan(&items);
            if let Some(resolution) = resolution {
                super::print_would_cache_root(resolution);
            }
            print_confirmation_hint(&hosts, report.needs_yes);
        }
        return exit(invalid);
    }

    apply_connections(
        prepared,
        &mut state,
        report,
        ApplyOptions {
            json: args.json,
            quiet: args.quiet,
        },
    )
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
    let ConnectManifest {
        manifest,
        resolution,
        _state_lock,
        ..
    } = match load_connect_manifest(&CliOverrides::default(), !args.dry_run, false) {
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
    // Removing an entry that an unselected host also records would break it.
    // A missing host directory cannot share anything, so grouping is skipped.
    let shared = plan::group_destinations(&manifest, &state, &mut items).unwrap_or_default();
    if args.dry_run {
        print_write_refusal(resolution.write_refusal().as_deref());
        println!("Dry run — planned actions (nothing written):");
        print_plan(&items);
        super::print_would_cache_root(&resolution);
        print_confirmation_hint(&hosts, hosts.len() > 1 && !args.yes);
        return exit(shared.incompatible);
    }
    if shared.incompatible {
        print_plan_results_noop(&items);
        return ExitCode::from(1);
    }
    if !confirm_multi_host_write(&hosts, args.yes) {
        return ExitCode::from(2);
    }
    remember_write_root(Some(&resolution), &items);
    let mut warned = false;
    let mut failed = false;
    for item in &items {
        match item.action {
            Action::Remove => match apply::apply_prepared(item, &mut state, None) {
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
    let ConnectManifest {
        manifest,
        resolution,
        _state_lock,
        ..
    } = match load_connect_manifest(&CliOverrides::default(), !args.dry_run, false) {
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
    let mut items = match plan::plan_convert(&manifest, &state, &skills, &active_hosts, to) {
        Ok(items) => items,
        Err(e) => return config_error(e),
    };
    let shared = match plan::group_destinations(&manifest, &state, &mut items) {
        Ok(shared) => shared,
        Err(err) => return config_error(err),
    };
    if shared.incompatible && !args.dry_run {
        print_plan_results_noop(&items);
        return ExitCode::from(1);
    }
    if args.dry_run {
        print_write_refusal(resolution.write_refusal().as_deref());
        println!("Dry run — planned actions (nothing written):");
        print_plan(&items);
        print_skipped_convert_hosts(&skipped_hosts);
        super::print_would_cache_root(&resolution);
        print_confirmation_hint(&active_hosts, active_hosts.len() > 1 && !args.yes);
        if shared.incompatible {
            return ExitCode::from(1);
        }
    } else {
        if !confirm_multi_host_write(&active_hosts, args.yes) {
            return ExitCode::from(2);
        }
        print_skipped_convert_hosts(&skipped_hosts);
    }
    let prepared = PreparedPlan::new(&items);
    let checks = check_targets(
        &manifest,
        &skills,
        &active_hosts,
        &prepared,
        CheckScope::ConvertCopies,
    );
    let validation = validate_selected(&checks, Some(&prepared));
    print_validation(&validation, false);
    if validation.iter().any(|check| !check.ok) {
        return ExitCode::from(1);
    }
    if args.dry_run {
        return ExitCode::SUCCESS;
    }
    remember_write_root(Some(&resolution), &items);
    let batch = apply_plan(
        prepared.with_groups(shared.groups),
        &mut state,
        false,
        false,
    );
    exit(!batch.succeeded())
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
    let writes = !args.dry_run && (!args.skip_skill_install || !args.no_hooks);
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
        if !args.no_hooks {
            super::hooks::preview_setup(&resolution.root);
        }
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
        if let Err(err) = resolution.recheck_write() {
            return config_error(err);
        }
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
        let code = sync_manifest_with_resolution(&manifest, &sync_args, Some(&resolution), None);
        if code != ExitCode::SUCCESS {
            return code;
        }
    }
    if !args.no_hooks {
        if let Err(error) = resolution.recheck_write() {
            return config_error(error);
        }
        match crate::git::hooks::Git::discover(&resolution.root) {
            Ok(git) => {
                if super::hooks::fill_for_setup(&git) {
                    resolution.remember();
                }
            }
            Err(_) => println!(
                "Git hooks: skipped ({})",
                super::hooks::NOT_A_GIT_REPOSITORY
            ),
        }
    }
    println!("Bootstrap complete. Run `my-skills doctor` to inspect registry configuration.");
    ExitCode::SUCCESS
}

// ---------------------------------------------------------------- shared

struct ConnectManifest {
    manifest: Manifest,
    resolution: root::RootResolution,
    _state_lock: Option<StateLock>,
    refusal: Option<String>,
}

fn load_connect_manifest(
    cli: &CliOverrides,
    writes: bool,
    report_refusal: bool,
) -> crate::Result<ConnectManifest> {
    if !writes {
        let (manifest, resolution) = root::load_manifest_resolved(cli, false)?;
        return Ok(ConnectManifest {
            manifest,
            resolution,
            _state_lock: None,
            refusal: None,
        });
    }
    let resolution = root::resolve_root(None, false)?;
    let mut refusal = resolution.write_refusal();
    if !report_refusal && let Some(message) = refusal.take() {
        return Err(Error::Manifest(message));
    }
    // An inactive cwd is only inspected for its refusal report.
    let state_lock = lock_state(refusal.is_none())?;
    if state_lock.is_some()
        && let Err(error) = resolution.recheck_write()
    {
        if !report_refusal {
            return Err(error);
        }
        refusal = Some(error.to_string());
    }
    let manifest = load_manifest(&resolution.root, cli)?;
    Ok(ConnectManifest {
        manifest,
        resolution,
        _state_lock: state_lock,
        refusal,
    })
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
fn remember_write_root(resolution: Option<&root::RootResolution>, items: &[PlanItem]) {
    if let Some(resolution) = resolution
        && items.iter().any(|item| item.action.writes())
    {
        resolution.remember();
    }
}

#[derive(Clone, Serialize)]
struct ValidationCheck<'a> {
    skill: &'a str,
    host: Option<&'a str>,
    ok: bool,
    errors: Vec<String>,
    warnings: Vec<String>,
}

impl<'a> ValidationCheck<'a> {
    fn new(skill: &'a str, host: Option<&'a str>, result: ValidationResult) -> Self {
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
    groups: BTreeMap<usize, Vec<usize>>,
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
            groups: BTreeMap::new(),
        }
    }

    fn with_groups(mut self, groups: BTreeMap<usize, Vec<usize>>) -> Self {
        self.groups = groups;
        self
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
    prepared: &PreparedPlan<'_>,
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
                CheckScope::ConvertCopies => prepared.items.iter().any(|item| {
                    item.skill == skill.name
                        && item.host == *host
                        && item.action == Action::Convert
                        && item.mode == InstallMode::Copy
                }),
            };
            if !eligible {
                continue;
            }
            if let Some(path) = prepared.snapshot(&skill.name, host) {
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

fn validate_selected<'a>(
    targets: &[CheckTarget<'a>],
    prepared: Option<&PreparedPlan<'a>>,
) -> Vec<ValidationCheck<'a>> {
    let mut checks = Vec::new();
    for target in targets {
        checks.push(ValidationCheck::new(
            target.skill,
            None,
            validate_skill(&target.dir),
        ));
        for host in target.hosts.iter().filter_map(|host| get_host(host)) {
            checks.push(ValidationCheck::new(
                target.skill,
                Some(host.name),
                validate_skill_for_host(&target.dir, host),
            ));
        }
    }
    if let Some(prepared) = prepared {
        for (&(skill, host), connection) in &prepared.connections {
            if let Err(error) = connection {
                checks.push(ValidationCheck {
                    skill,
                    host: Some(host),
                    ok: false,
                    errors: vec![format!("could not stage a copy: {error}")],
                    warnings: Vec::new(),
                });
            }
        }
    }
    drop_repeated_host_rows(unique_validation(checks))
}

/// A host row that only repeats the skill-level errors and warnings adds
/// nothing; keep it when it says something different, or when it was clean.
fn drop_repeated_host_rows(rows: Vec<ValidationCheck<'_>>) -> Vec<ValidationCheck<'_>> {
    let skill_level: Vec<_> = rows
        .iter()
        .filter(|row| row.host.is_none())
        .map(|row| (row.skill, row.errors.clone(), row.warnings.clone()))
        .collect();
    rows.into_iter()
        .filter_map(|mut row| {
            if row.host.is_some()
                && let Some((_, errors, warnings)) =
                    skill_level.iter().find(|(skill, ..)| *skill == row.skill)
            {
                let said_something = !row.errors.is_empty() || !row.warnings.is_empty();
                row.errors.retain(|error| !errors.contains(error));
                row.warnings.retain(|warning| !warnings.contains(warning));
                if said_something && row.errors.is_empty() && row.warnings.is_empty() {
                    return None;
                }
            }
            Some(row)
        })
        .collect()
}

fn unique_validation<'a>(
    checks: impl IntoIterator<Item = ValidationCheck<'a>>,
) -> Vec<ValidationCheck<'a>> {
    let mut unique: Vec<ValidationCheck<'_>> = Vec::new();
    for check in checks {
        if let Some(existing) = unique
            .iter_mut()
            .find(|row| row.skill == check.skill && row.host == check.host)
        {
            existing.ok &= check.ok;
            for error in check.errors {
                if !existing.errors.contains(&error) {
                    existing.errors.push(error);
                }
            }
            for warning in check.warnings {
                if !existing.warnings.contains(&warning) {
                    existing.warnings.push(warning);
                }
            }
        } else {
            unique.push(check);
        }
    }
    unique
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
    let mut printed = BTreeSet::new();
    for check in checks {
        if !check.ok {
            invalid = true;
            let errors: Vec<_> = check
                .errors
                .iter()
                .filter(|error| printed.insert((check.skill, *error)))
                .collect();
            if !errors.is_empty() {
                let host = check.host.map_or(String::new(), |host| format!("{host} "));
                print(&format!(
                    "[BLOCKED] {}: {host}validation failed",
                    check.skill
                ));
                for error in errors {
                    print(&format!("  error: {error}"));
                }
            }
        }
        // With --json the warnings are already in `validation[].warnings`.
        if !json {
            for warning in &check.warnings {
                if printed.insert((check.skill, warning)) {
                    print(&format!("  warn: {}: {warning}", check.skill));
                }
            }
        }
    }
    if invalid {
        eprintln!("\nNo files were changed (fix validation errors first).");
    }
}

struct ApplyOptions {
    json: bool,
    quiet: bool,
}

fn apply_connections<'a>(
    mut prepared: PreparedPlan<'a>,
    state: &mut State,
    report: PlanReport<'a>,
    output: ApplyOptions,
) -> ExitCode {
    let checks = prepared.prepare_links();
    let fallback_validation = validate_selected(&checks, None);
    print_validation(&fallback_validation, output.json);
    let invalid = fallback_validation.iter().any(|check| !check.ok);
    let validation;
    let report = if output.json && !checks.is_empty() {
        validation =
            unique_validation(report.validation.iter().cloned().chain(fallback_validation));
        PlanReport {
            validation: &validation,
            ..report
        }
    } else {
        report
    };
    if invalid {
        return report.stop(BatchStop::Blocked, output.json);
    }

    remember_write_root(report.resolution, report.items);
    let batch = apply_plan(prepared, state, output.quiet, output.json);
    if output.json {
        report.print_applied(&batch, None);
    }
    exit(!batch.succeeded())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum ApplyStatus {
    Created,
    Updated,
    Removed,
    Unchanged,
    Blocked,
    Failed,
    Skipped,
}

#[derive(Clone)]
struct ItemResult {
    status: ApplyStatus,
    mode: Option<InstallMode>,
    fallback_reason: Option<String>,
    error: Option<String>,
}

impl ItemResult {
    fn new(status: ApplyStatus, mode: Option<InstallMode>, error: Option<String>) -> Self {
        Self {
            status,
            mode,
            error,
            fallback_reason: None,
        }
    }

    fn applied(item: &PlanItem, outcome: &Outcome) -> Self {
        let status = match outcome {
            Outcome::Created(_) => ApplyStatus::Created,
            Outcome::Updated | Outcome::Converted(_) => ApplyStatus::Updated,
            Outcome::Removed => ApplyStatus::Removed,
            Outcome::Unchanged => ApplyStatus::Unchanged,
        };
        let mut result = Self::new(status, Some(item.mode), None);
        if let Outcome::Created(Connection::CopyFallback { reason }) = outcome {
            result.mode = Some(InstallMode::Copy);
            result.fallback_reason = Some(reason.clone());
        }
        result
    }
}

#[derive(Default, Serialize)]
struct ApplySummary {
    created: usize,
    updated: usize,
    removed: usize,
    unchanged: usize,
    blocked: usize,
    failed: usize,
}

#[derive(Serialize)]
struct CleanupNotice<'a> {
    skill: &'a str,
    host: &'a str,
    path: String,
    message: String,
}

struct BatchResult<'a> {
    results: Vec<ItemResult>,
    cleanup_warnings: Vec<CleanupNotice<'a>>,
}

impl<'a> BatchResult<'a> {
    fn from_plan(items: &[PlanItem], stopped: bool) -> Self {
        let results = items
            .iter()
            .map(|item| {
                if item.action.is_block() {
                    ItemResult::new(ApplyStatus::Blocked, None, Some(item.reason.clone()))
                } else if stopped && item.action.writes() {
                    ItemResult::new(ApplyStatus::Blocked, None, None)
                } else if !stopped && item.action == Action::Noop {
                    ItemResult::new(ApplyStatus::Unchanged, Some(item.mode), None)
                } else {
                    ItemResult::new(ApplyStatus::Skipped, None, None)
                }
            })
            .collect();
        Self {
            results,
            cleanup_warnings: Vec::new(),
        }
    }

    fn succeeded(&self) -> bool {
        self.results
            .iter()
            .all(|result| !matches!(result.status, ApplyStatus::Blocked | ApplyStatus::Failed))
    }

    fn summary(&self) -> ApplySummary {
        let mut summary = ApplySummary::default();
        for result in &self.results {
            match result.status {
                ApplyStatus::Created => summary.created += 1,
                ApplyStatus::Updated => summary.updated += 1,
                ApplyStatus::Removed => summary.removed += 1,
                ApplyStatus::Unchanged => summary.unchanged += 1,
                ApplyStatus::Blocked => summary.blocked += 1,
                ApplyStatus::Failed => summary.failed += 1,
                ApplyStatus::Skipped => {}
            }
        }
        summary
    }
}

/// Apply once per physical destination. The same results drive the text
/// lines and the machine-readable batch report.
fn apply_plan<'a>(
    mut plan: PreparedPlan<'a>,
    state: &mut State,
    quiet: bool,
    json: bool,
) -> BatchResult<'a> {
    let mut batch = BatchResult::from_plan(plan.items, false);
    let aliases: BTreeSet<_> = plan.groups.values().flatten().copied().collect();
    for (index, item) in plan.items.iter().enumerate() {
        if aliases.contains(&index) {
            continue;
        }
        if !item.action.writes() {
            if item.action == Action::Noop {
                batch.results[index].fallback_reason = state
                    .get(&item.skill, &item.host)
                    .and_then(|record| record.fallback_reason.clone());
            }
            if !json && (item.action.is_block() || !quiet) {
                print_result_line(item, None);
            }
            continue;
        }
        let peers = plan.groups.remove(&index).unwrap_or_default();
        let peer_items: Vec<_> = peers.iter().map(|&idx| &plan.items[idx]).collect();
        let prepared = plan
            .connections
            .remove(&(item.skill.as_str(), item.host.as_str()))
            .expect("every write has a preparation result");
        let result = prepared.and_then(|connection| {
            apply::apply_prepared_group(item, &peer_items, state, connection.as_ref())
        });
        let row = match result {
            Ok(applied) => {
                if !json {
                    for applied_item in std::iter::once(item).chain(peer_items) {
                        print_result_line(applied_item, Some(&applied.outcome));
                    }
                }
                applied.print_cleanup_warnings();
                batch
                    .cleanup_warnings
                    .extend(
                        applied
                            .cleanup_warnings
                            .iter()
                            .map(|warning| CleanupNotice {
                                skill: &item.skill,
                                host: &item.host,
                                path: display_path(&warning.path),
                                message: warning.to_string(),
                            }),
                    );
                ItemResult::applied(item, &applied.outcome)
            }
            Err(error) => {
                let status = if matches!(error, Error::CopyConflict(_)) {
                    ApplyStatus::Blocked
                } else {
                    ApplyStatus::Failed
                };
                if !json {
                    match &error {
                        Error::CopyConflict(path) => println!(
                            "  CONFLICT: {} -> {} ({path} changed locally after planning)",
                            item.skill, item.host
                        ),
                        Error::StateSave(e) => {
                            println!("  FAILED: {} -> {} (state: {e})", item.skill, item.host)
                        }
                        _ => println!("  FAILED: {} -> {} ({error})", item.skill, item.host),
                    }
                }
                ItemResult::new(status, None, Some(error.to_string()))
            }
        };
        for target in std::iter::once(index).chain(peers) {
            batch.results[target] = row.clone();
        }
    }
    batch
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

struct PlanReport<'a> {
    items: &'a [PlanItem],
    validation: &'a [ValidationCheck<'a>],
    resolution: Option<&'a root::RootResolution>,
    needs_yes: bool,
}

impl<'a> PlanReport<'a> {
    fn unchecked(
        items: &'a [PlanItem],
        resolution: Option<&'a root::RootResolution>,
        needs_yes: bool,
    ) -> Self {
        Self {
            items,
            validation: &[],
            resolution,
            needs_yes,
        }
    }

    fn print_preview(&self, json: bool) {
        let refusal = self
            .resolution
            .and_then(root::RootResolution::write_refusal);
        if json {
            print_json(&self.json_payload(refusal.as_deref()));
        } else {
            print_write_refusal(refusal.as_deref());
            let validation = self.validation.iter().filter(|check| !check.ok).count();
            let plan = self
                .items
                .iter()
                .filter(|item| item.action.is_block())
                .count();
            let count = validation + plan;
            if count != 0 {
                println!(
                    "BLOCKED: {count} item(s) would be refused (validation {validation}, plan {plan})"
                );
            }
        }
        print_validation(self.validation, json);
    }

    fn json_payload(&self, refusal: Option<&str>) -> Value {
        let mut reasons = Vec::new();
        if let Some(message) = refusal {
            reasons.push(block_reason(None, None, "inactive_registry", message));
        }
        reasons.extend(
            self.validation
                .iter()
                .filter(|check| !check.ok)
                .map(|check| {
                    block_reason(
                        Some(check.skill),
                        check.host,
                        "validation",
                        &check.errors.join("\n"),
                    )
                }),
        );
        reasons.extend(
            self.items
                .iter()
                .filter(|item| item.action.is_block())
                .map(|item| {
                    block_reason(
                        Some(&item.skill),
                        Some(&item.host),
                        &item.action.label().to_ascii_lowercase(),
                        &item.reason,
                    )
                }),
        );
        let blocked = !reasons.is_empty();
        if self.needs_yes {
            reasons.push(block_reason(
                None,
                None,
                "needs_yes",
                "this command writes to multiple hosts and requires --yes",
            ));
        }
        json!({
            "actions": self.items.iter().map(plan_row).collect::<Vec<_>>(),
            "registry": self.resolution.map(root::RootResolution::meta),
            "dry_run": true,
            "blocked": blocked,
            "applicable": !blocked && !self.needs_yes,
            "block_reasons": reasons,
            "validation": self.validation,
        })
    }

    fn stop(&self, reason: BatchStop<'_>, json: bool) -> ExitCode {
        let code = match reason {
            BatchStop::Blocked => ExitCode::from(1),
            BatchStop::NeedsYes => ExitCode::from(2),
            BatchStop::Inactive(message) => {
                eprintln!("error: {message}");
                ExitCode::from(2)
            }
        };
        if json {
            let batch = BatchResult::from_plan(self.items, true);
            self.print_applied(&batch, Some(reason));
        }
        code
    }

    fn print_applied(&self, batch: &BatchResult, stop: Option<BatchStop<'_>>) {
        let refusal = stop.and_then(|stop| match stop {
            BatchStop::Inactive(message) => Some(message),
            _ => None,
        });
        let mut payload = self.json_payload(refusal);
        let rows = payload["actions"]
            .as_array_mut()
            .expect("actions are an array");
        let mut late_conflicts = Vec::new();
        for ((row, item), result) in rows.iter_mut().zip(self.items).zip(&batch.results) {
            row["requested_mode"] = json!(mode_label(item.mode));
            row["mode"] = json!(result.mode.map(mode_label));
            row["result"] = json!(result.status);
            row["error"] = json!(result.error);
            row["fallback_reason"] = json!(result.fallback_reason);
            if stop.is_none()
                && result.status == ApplyStatus::Blocked
                && !item.action.is_block()
                && let Some(error) = &result.error
            {
                late_conflicts.push(block_reason(
                    Some(&item.skill),
                    Some(&item.host),
                    "conflict",
                    error,
                ));
            }
        }
        if !late_conflicts.is_empty() {
            payload["blocked"] = json!(true);
            payload["applicable"] = json!(false);
            payload["block_reasons"]
                .as_array_mut()
                .expect("block reasons are an array")
                .extend(late_conflicts);
        }
        payload["dry_run"] = json!(false);
        payload["applied"] = json!(stop.is_none() && batch.succeeded());
        payload["summary"] = json!(batch.summary());
        payload["cleanup_warnings"] = json!(batch.cleanup_warnings);
        print_json(&payload);
    }
}

#[derive(Clone, Copy)]
enum BatchStop<'a> {
    Blocked,
    NeedsYes,
    Inactive(&'a str),
}

fn block_reason(skill: Option<&str>, host: Option<&str>, kind: &str, message: &str) -> Value {
    json!({"skill": skill, "host": host, "kind": kind, "message": message})
}

fn plan_row(item: &PlanItem) -> Value {
    json!({
        "skill": item.skill,
        "host": item.host,
        "action": item.action.label(),
        "reason": item.reason,
        "mode": mode_label(item.mode),
        "source": display_path(&item.source),
        "destination": display_path(&item.destination),
    })
}

fn print_json(payload: &Value) {
    println!(
        "{}",
        serde_json::to_string_pretty(payload).expect("JSON values always serialize")
    );
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
        let items = std::slice::from_ref(&item);
        let applied = apply_plan(PreparedPlan::new(items), &mut state, false, false);
        assert!(applied.succeeded());
        assert_eq!(applied.summary().created, 1);
        assert_eq!(State::load(Some(&state_path)).unwrap(), state);

        item.action = Action::Convert;
        for mode in [InstallMode::Link, InstallMode::Copy] {
            item.mode = mode;
            let items = std::slice::from_ref(&item);
            let applied = apply_plan(PreparedPlan::new(items), &mut state, false, false);
            assert!(applied.succeeded());
            assert_eq!(applied.summary().updated, 1);
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
