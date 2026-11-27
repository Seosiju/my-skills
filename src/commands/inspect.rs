//! validate, set-root, data-path.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde_json::json;

use super::config_error;
use crate::check::validate::gate_validation;
use crate::cli::*;
use crate::platform::data::skill_data_path;
use crate::platform::paths;
use crate::registry::manifest::{CliOverrides, Manifest, select_names};
use crate::registry::root;

fn skill_dirs(manifest: &Manifest) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(manifest.skills_dir()) else {
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
    let (manifest, resolution) = match root::load_manifest_resolved(&CliOverrides::default(), false)
    {
        Ok(loaded) => loaded,
        Err(e) => return config_error(e),
    };
    let host = args.host.as_deref();
    if let Some(host) = host.filter(|host| !manifest.targets.contains_key(*host)) {
        return config_error(format!("unknown host: {host}"));
    }
    let dirs = if args.skills.is_empty() && args.groups.is_empty() {
        skill_dirs(&manifest)
    } else {
        // Authors can validate draft directories before registering them.
        let (drafts, requested): (Vec<_>, Vec<_>) = args.skills.iter().cloned().partition(|name| {
            !manifest.skills.contains_key(name)
                && Path::new(name)
                    .file_name()
                    .is_some_and(|part| part == name.as_str())
                && manifest.skill_dir(name).is_dir()
        });
        let mut names = match select_names(&manifest, &requested, &args.groups) {
            Ok(names) => names,
            Err(e) => return config_error(e),
        };
        names.extend(drafts);
        names.sort_unstable();
        names.dedup();
        names.iter().map(|name| manifest.skill_dir(name)).collect()
    };
    let mut ok = true;
    let mut skills = Vec::new();
    if dirs.is_empty() && !args.json {
        println!("no skills found to validate");
    }
    for dir in dirs {
        let name = dir.file_name().unwrap_or_default().to_string_lossy();
        let result = gate_validation(&dir, &name, Some(&manifest), host);
        ok &= result.ok();
        if args.json {
            skills.push(json!({
                "skill": result.skill,
                "ok": result.ok(),
                "errors": result.errors,
                "warnings": result.warnings,
            }));
        } else {
            let status = if result.ok() { "OK" } else { "FAIL" };
            println!("[{status}] {}", result.skill);
            for error in &result.errors {
                println!("  error: {error}");
            }
            for warning in &result.warnings {
                println!("  warn:  {warning}");
            }
        }
    }
    if args.json {
        let report = json!({"registry": resolution.meta(), "ok": ok, "skills": skills});
        println!("{report:#}");
    }
    ExitCode::from(u8::from(!ok))
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
