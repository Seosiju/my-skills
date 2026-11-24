//! install, sync, uninstall, convert, bootstrap. Integrator.
//!
//! Output lines and exit codes follow v0.4.1 `install_commands.py` and
//! `bootstrap_commands.py`. v1 changes: links are the default mode, sync
//! reconciles (connects and prunes, D3), and `convert` is new (spec §6.6).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;

use serde_json::{Value, json};

use super::config_error;
use crate::check::audit::{
    AuditGateResult, audit_skills, format_gate, gate_json, policy_from_settings,
};
use crate::check::validate::{compose_validation, validate_skill_for_host};
use crate::cli::*;
use crate::drive::apply::{self, AuditMeta, Connection, Outcome};
use crate::drive::hosts::{self, get_host};
use crate::drive::link::LinkKind;
use crate::drive::plan::{self, Action, PlanItem, SyncScope};
use crate::drive::state::State;
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
    let manifest = match root::load_manifest_from_cwd(&cli) {
        Ok(m) => m,
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
    if !validate_selected(&manifest, &skills, &hosts) {
        eprintln!("\nNo files were changed (fix validation errors first).");
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
        if args.json {
            let mut payload = plan_json(&items);
            payload["audit"] = gate_json(&gate);
            print_json(&payload);
            return ExitCode::SUCCESS;
        }
        println!("{}", format_gate(&gate, "AUDIT WOULD BLOCK"));
        println!("Dry run — planned actions (nothing written):");
        print_plan(&items);
        return ExitCode::SUCCESS;
    }

    let allow = is_builtin_seed_default_install(args, &manifest, &skills, &hosts);
    if !allow && !confirm_multi_host_write(&hosts, args.yes) {
        return ExitCode::from(2);
    }
    if let Some(code) = report_gate(&gate) {
        return code;
    }
    let audit = audit_meta_by_skill(&gate);
    let (_, blocked) = apply_plan(&items, &mut state, &audit, false, State::save);
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
    let manifest = match root::load_manifest_from_cwd(&CliOverrides::default()) {
        Ok(m) => m,
        Err(e) => return config_error(e),
    };
    sync_manifest(&manifest, args)
}

/// Sync against an already loaded manifest (also used by bootstrap and the
/// git hooks).
pub fn sync_manifest(manifest: &Manifest, args: &SyncArgs) -> ExitCode {
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

    let connecting: Vec<&Skill> = skills
        .iter()
        .copied()
        .filter(|s| {
            items
                .iter()
                .any(|i| i.skill == s.name && matches!(i.action, Action::Create | Action::Update))
        })
        .collect();

    if args.dry_run {
        println!("Dry run — planned actions (nothing written):");
        print_plan(&items);
        return ExitCode::SUCCESS;
    }
    if !items.iter().any(|i| i.action.writes()) {
        if !args.quiet {
            print_plan_results_noop(&items);
        }
        return exit(items.iter().any(|i| i.action.is_block()));
    }
    if !confirm_multi_host_write(&hosts, args.yes) {
        return ExitCode::from(2);
    }
    if !validate_selected(manifest, &connecting, &hosts) {
        eprintln!("\nNo files were changed (fix validation errors first).");
        return ExitCode::from(1);
    }
    let gate = match audit_gate(manifest, &connecting, args.skip_audit) {
        Ok(gate) => gate,
        Err(e) => return config_error(e),
    };
    if !args.quiet || gate.blocked() {
        if let Some(code) = report_gate(&gate) {
            return code;
        }
    }
    let mut state = state;
    let audit = audit_meta_by_skill(&gate);
    let (_, blocked) = apply_plan(&items, &mut state, &audit, args.quiet, State::save);
    exit(blocked)
}

fn print_plan_results_noop(items: &[PlanItem]) {
    for item in items {
        print_result_line(item, None);
    }
}

// ---------------------------------------------------------------- uninstall

pub fn uninstall(args: &UninstallArgs) -> ExitCode {
    let manifest = match root::load_manifest_from_cwd(&CliOverrides::default()) {
        Ok(m) => m,
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
                Ok(_) => {
                    if let Err(e) = state.save() {
                        return config_error(e);
                    }
                    println!("  removed: {} -> {}", item.skill, item.host);
                }
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
                println!("    {}", item.destination.display());
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
    let manifest = match root::load_manifest_from_cwd(&CliOverrides::default()) {
        Ok(m) => m,
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
        return ExitCode::SUCCESS;
    }
    if !confirm_multi_host_write(&active_hosts, args.yes) {
        return ExitCode::from(2);
    }
    print_skipped_convert_hosts(&skipped_hosts);
    let (_, blocked) = apply_plan(&items, &mut state, &BTreeMap::new(), false, State::save);
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
        println!("  then: my-skills hooks install");
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

fn validate_selected(manifest: &Manifest, skills: &[&Skill], hosts_: &[String]) -> bool {
    let mut ok = true;
    for skill in skills {
        let dir = manifest.skill_dir(&skill.name);
        let result = compose_validation(&dir);
        if !result.ok() {
            ok = false;
            println!("[BLOCKED] {}: validation failed", skill.name);
            for error in &result.errors {
                println!("  error: {error}");
            }
        }
        for host in hosts_ {
            // A host the skill does not connect to cannot reject it.
            if !skill.hosts.is_empty() && !skill.hosts.iter().any(|h| h == host) {
                continue;
            }
            let Some(config) = get_host(host) else {
                continue;
            };
            let host_result = validate_skill_for_host(&dir, config);
            if !host_result.ok() {
                ok = false;
                println!("[BLOCKED] {}: {host} validation failed", skill.name);
                for error in &host_result.errors {
                    println!("  error: {error}");
                }
            }
            for warning in &host_result.warnings {
                println!("  warn: {warning}");
            }
        }
    }
    ok
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

/// Apply writes in order and print v0.4.1-style result lines. Save state for
/// changes that `apply` did not already save. Returns (changed, blocked).
fn apply_plan(
    items: &[PlanItem],
    state: &mut State,
    audit: &BTreeMap<String, AuditMeta>,
    quiet: bool,
    save: impl Fn(&State) -> crate::Result<()>,
) -> (usize, bool) {
    let mut changed = 0;
    let mut blocked = false;
    for item in items {
        if item.action.writes() {
            match apply::apply(item, state, audit.get(&item.skill)) {
                Ok(outcome) => {
                    let save_result = match outcome {
                        Outcome::Converted(_) => Ok(()),
                        _ => save(state),
                    };
                    if let Err(e) = save_result {
                        blocked = true;
                        println!("  FAILED: {} -> {} (state: {e})", item.skill, item.host);
                        continue;
                    }
                    changed += 1;
                    print_result_line(item, Some(&outcome));
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
            println!("    {}", item.destination.display());
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

fn plan_json(items: &[PlanItem]) -> Value {
    json!({
        "actions": items.iter().map(|item| json!({
            "skill": item.skill,
            "host": item.host,
            "action": item.action.label(),
            "reason": item.reason,
            "mode": mode_label(item.mode),
            "source": item.source.to_string_lossy(),
            "destination": item.destination.to_string_lossy(),
        })).collect::<Vec<_>>()
    })
}

fn print_json(value: &Value) {
    println!(
        "{}",
        serde_json::to_string_pretty(value).expect("JSON values always serialize")
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
        let audit = BTreeMap::new();
        let items = std::slice::from_ref(&item);
        assert_eq!(
            apply_plan(items, &mut state, &audit, false, State::save),
            (1, false)
        );
        assert_eq!(State::load(Some(&state_path)).unwrap(), state);

        item.action = Action::Convert;
        for mode in [InstallMode::Link, InstallMode::Copy] {
            item.mode = mode;
            let items = std::slice::from_ref(&item);
            assert_eq!(
                apply_plan(items, &mut state, &audit, false, |_| {
                    panic!("a successful conversion must save its own record")
                }),
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
