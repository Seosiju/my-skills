//! validate, audit, status, set-root, data-path. Integrator.
//! Output follows v0.4.1 `inspection_commands.py`, `audit_commands.py`,
//! `cli_runtime.cmd_set_root`, and `registry_commands.cmd_data_path`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde_json::json;

use super::config_error;
use crate::check::audit::{
    AuditFinding, Severity, cross_skill_findings, finding_json, policy_from_settings, result_json,
    run_audit,
};
use crate::check::validate::compose_validation;
use crate::cli::*;
use crate::drive::link::{self, EntryKind};
use crate::drive::plan;
use crate::drive::state::State;
use crate::platform::data::skill_data_path;
use crate::platform::paths;
use crate::registry::manifest::{CliOverrides, InstallMode, Manifest};
use crate::registry::root;

fn load() -> Result<Manifest, ExitCode> {
    root::load_manifest_from_cwd(&CliOverrides::default()).map_err(config_error)
}

fn skill_dirs(manifest: &Manifest, skill: Option<&str>) -> Vec<PathBuf> {
    let skills_dir = manifest.skills_dir();
    if let Some(name) = skill {
        return vec![skills_dir.join(name)];
    }
    let Ok(entries) = fs::read_dir(&skills_dir) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    dirs
}

pub fn validate(args: &ValidateArgs) -> ExitCode {
    let manifest = match load() {
        Ok(m) => m,
        Err(code) => return code,
    };
    let requested = match super::one_skill(&args.skills, &args.groups) {
        Ok(requested) => requested,
        Err(e) => return config_error(e),
    };
    let dirs = skill_dirs(&manifest, requested);
    if dirs.is_empty() {
        println!("no skills found to validate");
        return ExitCode::SUCCESS;
    }
    let mut had_error = false;
    for dir in dirs {
        let result = compose_validation(&dir);
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        println!("[{}] {name}", if result.ok() { "OK" } else { "FAIL" });
        for error in &result.errors {
            println!("  error: {error}");
            had_error = true;
        }
        for warning in &result.warnings {
            println!("  warn:  {warning}");
        }
    }
    if had_error {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

pub fn audit(args: &AuditArgs) -> ExitCode {
    let manifest = match load() {
        Ok(m) => m,
        Err(code) => return code,
    };
    let requested =
        match crate::registry::manifest::select_names(&manifest, &args.skills, &args.groups) {
            Ok(requested) => requested,
            Err(e) => return config_error(e),
        };
    let skills = match crate::registry::manifest::selected_skills(&manifest, &requested, args.all) {
        Ok(selection) => selection,
        Err(e) => return config_error(e),
    };
    let policy = match policy_from_settings(&manifest.audit) {
        Ok(p) => p,
        Err(e) => return config_error(e),
    };
    let mut results = Vec::new();
    for skill in &skills {
        match run_audit(&manifest.skill_dir(&skill.name), &policy) {
            Ok(result) => results.push(result),
            Err(e) => return config_error(e),
        }
    }
    let bundle = cross_skill_findings(&results);
    let threshold = policy.effective_threshold();
    let bundle_blocked = bundle_blocked(&bundle, threshold);
    let blocked = results.iter().any(|r| r.blocked()) || bundle_blocked;

    if args.json {
        let value = if requested.len() == 1 {
            result_json(&results[0])
        } else {
            json!({
                "blocked": blocked,
                "bundle_findings": bundle.iter().map(finding_json).collect::<Vec<_>>(),
                "skills": results.iter().map(result_json).collect::<Vec<_>>(),
            })
        };
        println!(
            "{}",
            serde_json::to_string_pretty(&value).expect("JSON values always serialize")
        );
    } else {
        for result in &results {
            let status = if result.blocked() { "BLOCKED" } else { "OK" };
            let threshold = result.threshold.map_or("none", |t| t.label());
            println!("[{status}] {} (threshold={threshold})", result.skill);
            for finding in &result.findings {
                print_finding(finding);
            }
        }
        if !bundle.is_empty() {
            let status = if bundle_blocked { "BLOCKED" } else { "WARN" };
            println!("[{status}] selected skill bundle");
            for finding in &bundle {
                print_finding(finding);
            }
        }
    }
    if blocked {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn print_finding(finding: &AuditFinding) {
    println!(
        "  {}: {}: {}: {}",
        finding.severity.label(),
        finding.rule_id,
        finding.file,
        finding.message
    );
}

fn bundle_blocked(findings: &[AuditFinding], threshold: Option<Severity>) -> bool {
    threshold.is_some_and(|t| findings.iter().any(|f| f.severity >= t))
}

pub fn status(args: &StatusArgs) -> ExitCode {
    let manifest = match load() {
        Ok(m) => m,
        Err(code) => return code,
    };
    let state = match State::load(None) {
        Ok(s) => s,
        Err(e) => return config_error(e),
    };
    let hosts: Vec<&str> = manifest
        .targets
        .values()
        .filter(|t| t.enabled)
        .map(|t| t.name.as_str())
        .collect();
    if manifest.skills.is_empty() {
        if args.json {
            println!("[]");
        } else {
            println!("no skills registered in manifest");
        }
        return ExitCode::SUCCESS;
    }
    let mut rows = Vec::new();
    for skill in manifest.skills.values() {
        if !args.json {
            println!("{}", skill.name);
        }
        for host in &hosts {
            let status = match plan::status_of(&manifest, skill, host, &state) {
                Ok(s) => s,
                Err(e) => return config_error(e),
            };
            if args.json {
                let record = state.get(&skill.name, host);
                let link_kind = plan::destination(&manifest, host, &skill.name)
                    .ok()
                    .and_then(|dest| match link::entry_kind(&dest) {
                        Ok(EntryKind::Link(kind)) => Some(kind),
                        _ => None,
                    });
                rows.push(json!({
                    "skill": skill.name,
                    "host": host,
                    "status": status.label(),
                    "mode": record.map(|r| match r.mode {
                        InstallMode::Link => "link",
                        InstallMode::Copy => "copy",
                    }),
                    "link_kind": link_kind,
                    "fallback_reason": record.and_then(|r| r.fallback_reason.clone()),
                }));
            } else {
                println!("  {host:8} {}", status.label());
            }
        }
    }
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&rows).expect("JSON values always serialize")
        );
    }
    ExitCode::SUCCESS
}

pub fn set_root(args: &SetRootArgs) -> ExitCode {
    let record = |path: &Path| {
        if args.dry_run {
            root::validate_repo_root(path)
        } else {
            root::cache_repo_root(path)
        }
    };
    let root = match &args.path {
        Some(path) => record(Path::new(path)),
        None => root::find_cwd_root(None).and_then(|p| record(&p)),
    };
    match root {
        Ok(root) if args.dry_run => {
            println!(
                "Dry run — would set active registry root to {}",
                paths::display_path(&root)
            );
            ExitCode::SUCCESS
        }
        Ok(root) => {
            println!("Active registry root set to {}", paths::display_path(&root));
            ExitCode::SUCCESS
        }
        Err(e) => config_error(e),
    }
}

pub fn data_path(args: &DataPathArgs) -> ExitCode {
    match skill_data_path(&args.skill, args.create) {
        Ok(path) => {
            println!("{}", paths::display_path(&path));
            ExitCode::SUCCESS
        }
        Err(e) => config_error(e),
    }
}
