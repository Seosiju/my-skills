//! install, sync, uninstall, convert, bootstrap. Integrator.
//!
//! Output lines and exit codes follow v0.4.1 `install_commands.py` and
//! `bootstrap_commands.py`. v1 changes: links are the default mode, sync
//! reconciles (connects and prunes, D3), and `convert` is new (spec §6.6).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;

use serde::Serialize;
use serde_json::json;

use super::config_error;
use crate::Error;
use crate::check::audit::{
    AuditGateResult, audit_skills, format_gate, gate_json, policy_from_settings,
};
use crate::check::validate::{ValidationResult, compose_validation, validate_skill_for_host};
use crate::cli::*;
use crate::drive::apply::{self, AuditMeta, Connection, Outcome};
use crate::drive::hosts::{self, get_host};
use crate::drive::link::LinkKind;
use crate::drive::plan::{self, Action, PlanItem, SyncScope};
use crate::drive::state::State;
use crate::platform::paths::display_path;
use crate::registry::manifest::{
    CliOverrides, InstallMode, Manifest, Skill, expand_path, load_manifest,
};
use crate::registry::root::{self, confirm_multi_host_write, resolve_hosts};

// ---------------------------------------------------------------- install

pub fn install(args: &InstallArgs) -> ExitCode {
    let cli = CliOverrides {
        install_mode: args.mode.map(Into::into),
        ..CliOverrides::default()
    };
    let (manifest, resolution) = match root::load_manifest_resolved(&cli, !args.dry_run) {
        Ok(loaded) => loaded,
        Err(e) => return config_error(e),
    };
    let (skills, hosts) = match root::select_requested(
        &manifest,
        args.skill.as_deref(),
        args.all,
        args.host.as_deref(),
    ) {
        Ok(selection) => selection,
        Err(e) => return config_error(e),
    };
    if args.json && !args.dry_run {
        return config_error("install --json requires --dry-run");
    }
    let validation = validate_selected(&manifest, &skills, &hosts);
    let allow = is_builtin_seed_default_install(args, &manifest, &skills, &hosts);
    let needs_yes = !allow && hosts.len() > 1 && !args.yes;
    if !args.dry_run {
        print_validation(&validation, false);
    }
    if validation.iter().any(|check| !check.ok) {
        if args.dry_run {
            DryRun {
                items: &[],
                validation: &validation,
                audit: None,
                resolution: Some(&resolution),
                needs_yes,
            }
            .print(args.json);
        }
        return ExitCode::from(1);
    }
    let gate = match audit_gate(&manifest, &skills, args.skip_audit) {
        Ok(gate) => gate,
        Err(e) => return config_error(e),
    };
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
        }
        return ExitCode::SUCCESS;
    }

    if !allow && !confirm_multi_host_write(&hosts, args.yes) {
        return ExitCode::from(2);
    }
    if let Some(code) = report_gate(&gate) {
        return code;
    }
    let audit = audit_meta_by_skill(&gate);
    let (_, blocked) = apply_plan(&items, &mut state, &audit, false);
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
    if args.skill.is_some()
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
    if args.json && !args.dry_run {
        return config_error("sync --json requires --dry-run");
    }
    let (manifest, resolution) = match root::load_manifest_resolved(
        &CliOverrides::default(),
        !args.dry_run && !args.check,
    ) {
        Ok(loaded) => loaded,
        Err(e) => return config_error(e),
    };
    sync_manifest_with_resolution(&manifest, args, Some(&resolution))
}

/// Sync against an already loaded manifest (also used by bootstrap and the
/// git hooks).
pub fn sync_manifest(manifest: &Manifest, args: &SyncArgs) -> ExitCode {
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
    // Prune needs disabled skills in scope too, so sync always looks at every
    // registered skill. `--all` keeps its v0.4.1 meaning: connect disabled
    // skills as well.
    let scoped: Vec<Skill> = match &args.skill {
        Some(name) => match manifest.skills.get(name) {
            Some(skill) => vec![skill.clone()],
            None => return config_error(format!("unknown skill(s): {name}")),
        },
        None => manifest.skills.values().cloned().collect(),
    };
    let scoped: Vec<Skill> = scoped
        .into_iter()
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
        include_orphans: args.skill.is_none(),
        prune: !args.no_prune,
    };
    let items = match plan::plan_sync(manifest, &state, &scope, None) {
        Ok(items) => items,
        Err(e) => return config_error(e),
    };

    if args.check && !args.json {
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
    if !args.dry_run {
        if !writes {
            if !args.quiet {
                print_plan_results_noop(&items);
            }
            return exit(items.iter().any(|i| i.action.is_block()));
        }
        if !confirm_multi_host_write(&hosts, args.yes) {
            return ExitCode::from(2);
        }
    }

    let connecting: Vec<&Skill> = skills
        .iter()
        .copied()
        .filter(|s| {
            items
                .iter()
                .any(|i| i.skill == s.name && matches!(i.action, Action::Create | Action::Update))
        })
        .collect();

    let validation = validate_selected(manifest, &connecting, &hosts);
    let invalid = validation.iter().any(|check| !check.ok);
    if !args.dry_run {
        print_validation(&validation, false);
        if invalid {
            return ExitCode::from(1);
        }
    }
    let gate = if writes && !invalid {
        match audit_gate(manifest, &connecting, args.skip_audit) {
            Ok(gate) => Some(gate),
            Err(e) => return config_error(e),
        }
    } else {
        None
    };

    if args.dry_run {
        DryRun {
            items: &items,
            validation: &validation,
            audit: gate.as_ref(),
            resolution,
            needs_yes: writes && hosts.len() > 1 && !args.yes,
        }
        .print(args.json);
        if !args.json {
            println!("Dry run — planned actions (nothing written):");
            print_plan(&items);
            if let Some(gate) = gate.as_ref().filter(|gate| gate.blocked()) {
                println!("{}", format_gate(gate, "AUDIT WOULD BLOCK"));
            }
            if let Some(resolution) = resolution {
                super::print_would_cache_root(resolution);
            }
        }
        return exit(invalid);
    }

    let gate = gate.expect("a real sync with writes and valid skills runs the audit gate");
    if !args.quiet || gate.blocked() {
        if let Some(code) = report_gate(&gate) {
            return code;
        }
    }
    let mut state = state;
    let audit = audit_meta_by_skill(&gate);
    let (_, blocked) = apply_plan(&items, &mut state, &audit, args.quiet);
    exit(blocked)
}

fn print_plan_results_noop(items: &[PlanItem]) {
    for item in items {
        print_result_line(item, None);
    }
}

// ---------------------------------------------------------------- uninstall

pub fn uninstall(args: &UninstallArgs) -> ExitCode {
    let (manifest, resolution) =
        match root::load_manifest_resolved(&CliOverrides::default(), !args.dry_run) {
            Ok(loaded) => loaded,
            Err(e) => return config_error(e),
        };
    let Some(skill) = args.skill.as_deref() else {
        return config_error("uninstall requires a skill name");
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
    for host in &hosts {
        match plan::plan_uninstall_one(&manifest, skill, host, &state) {
            Ok(item) => items.push(item),
            Err(e) => return config_error(e),
        }
    }
    if args.dry_run {
        println!("Dry run — planned actions (nothing written):");
        print_plan(&items);
        super::print_would_cache_root(&resolution);
        return ExitCode::SUCCESS;
    }
    if !confirm_multi_host_write(&hosts, args.yes) {
        return ExitCode::from(2);
    }
    let mut warned = false;
    let mut failed = false;
    for item in &items {
        match item.action {
            Action::Remove => match apply::apply(item, &mut state, None) {
                Ok(_) => println!("  removed: {} -> {}", item.skill, item.host),
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
    let (manifest, resolution) =
        match root::load_manifest_resolved(&CliOverrides::default(), !args.dry_run) {
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
        println!("Dry run — planned actions (nothing written):");
        print_plan(&items);
        print_skipped_convert_hosts(&skipped_hosts);
        super::print_would_cache_root(&resolution);
        return ExitCode::SUCCESS;
    }
    if !confirm_multi_host_write(&active_hosts, args.yes) {
        return ExitCode::from(2);
    }
    print_skipped_convert_hosts(&skipped_hosts);
    let (_, blocked) = apply_plan(&items, &mut state, &BTreeMap::new(), false);
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
    let root = match root::resolve_root(None, false) {
        Ok(r) => r.root,
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
        let manifest = match load_manifest(&root, &cli) {
            Ok(m) => m,
            Err(e) => return config_error(e),
        };
        let sync_args = SyncArgs {
            host: args.host.clone(),
            all: args.all,
            yes: true,
            ..SyncArgs::default()
        };
        let code = sync_manifest(&manifest, &sync_args);
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

fn validate_selected<'a>(
    manifest: &Manifest,
    skills: &[&'a Skill],
    hosts_: &[String],
) -> Vec<ValidationCheck<'a>> {
    let mut checks = Vec::new();
    for skill in skills {
        let dir = manifest.skill_dir(&skill.name);
        checks.push(ValidationCheck::new(
            &skill.name,
            None,
            compose_validation(&dir),
        ));
        for host in hosts_ {
            // A host the skill does not connect to cannot reject it.
            if !skill.hosts.is_empty() && !skill.hosts.iter().any(|h| h == host) {
                continue;
            }
            let Some(config) = get_host(host) else {
                continue;
            };
            checks.push(ValidationCheck::new(
                &skill.name,
                Some(config.name),
                validate_skill_for_host(&dir, config),
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
        if check.host.is_some() {
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
    skills: &[&Skill],
    skip: bool,
) -> crate::Result<AuditGateResult> {
    let policy = policy_from_settings(&manifest.audit)?;
    let dirs: Vec<PathBuf> = skills.iter().map(|s| manifest.skill_dir(&s.name)).collect();
    audit_skills(&dirs, &policy, skip)
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

fn audit_meta_by_skill(gate: &AuditGateResult) -> BTreeMap<String, AuditMeta> {
    let now = apply::utc_now();
    gate.results
        .iter()
        .map(|r| {
            (
                r.skill.clone(),
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
    items: &[PlanItem],
    state: &mut State,
    audit: &BTreeMap<String, AuditMeta>,
    quiet: bool,
) -> (usize, bool) {
    let mut changed = 0;
    let mut blocked = false;
    for item in items {
        if item.action.writes() {
            match apply::apply(item, state, audit.get(&item.skill)) {
                Ok(outcome) => {
                    changed += 1;
                    print_result_line(item, Some(&outcome));
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

struct DryRun<'a> {
    items: &'a [PlanItem],
    validation: &'a [ValidationCheck<'a>],
    audit: Option<&'a AuditGateResult>,
    resolution: Option<&'a root::RootResolution>,
    needs_yes: bool,
}

impl DryRun<'_> {
    fn print(&self, json: bool) {
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

        if json {
            let mut reasons: Vec<_> = validation
                .map(|check| {
                    json!({
                        "skill": check.skill,
                        "host": check.host,
                        "kind": "validation",
                        "message": check.errors.join("\n"),
                    })
                })
                .collect();
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
                skipped: true,
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
                "registry": self.resolution.map(|resolution| json!({
                    "path": display_path(&resolution.root),
                    "source": resolution.source.label(),
                })),
                "dry_run": true,
                "blocked": count != 0,
                "applicable": count == 0 && !self.needs_yes,
                "block_reasons": reasons,
                "validation": self.validation,
            });
            println!(
                "{}",
                serde_json::to_string_pretty(&payload).expect("JSON values always serialize")
            );
        } else if count != 0 {
            println!(
                "BLOCKED: {count} item(s) would be refused (validation {validation_count}, audit {audit_count}, plan {plan_count})"
            );
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
        assert_eq!(apply_plan(items, &mut state, &audit, false), (1, false));
        assert_eq!(State::load(Some(&state_path)).unwrap(), state);

        item.action = Action::Convert;
        for mode in [InstallMode::Link, InstallMode::Copy] {
            item.mode = mode;
            let items = std::slice::from_ref(&item);
            assert_eq!(apply_plan(items, &mut state, &audit, false), (1, false));
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
