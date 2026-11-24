//! init-registry, skills, share, enable, disable, import. Stream H.

use std::process::ExitCode;

use serde::Serialize;

use crate::check::audit::format_gate;
use crate::cli::*;
use crate::drive::state::State;
use crate::registry::catalog::{self, display_path, json_output};
use crate::registry::edit;
use crate::registry::manifest::{CliOverrides, MANIFEST_FILE, Manifest};
use crate::registry::seeds::{self, Scaffold};
use crate::registry::share::{self, Failure};
use crate::{Error, Result, registry::root};

pub fn init_registry(args: &InitRegistryArgs) -> ExitCode {
    let target = match seeds::registry_target(args.path.as_deref()) {
        Ok(target) => target,
        Err(err) => return super::config_error(err),
    };
    let scaffold = match Scaffold::check(&target) {
        Ok(scaffold) => scaffold,
        Err(Error::Refused(message)) => {
            eprintln!("error: {message}");
            return ExitCode::from(1);
        }
        Err(err) => return super::config_error(err),
    };
    let with_defaults = !args.no_defaults;
    if args.dry_run {
        println!(
            "Dry run — would create a private skill registry at {}",
            display_path(&target)
        );
        println!(
            "  seed skills: {}",
            if with_defaults { "yes" } else { "no" }
        );
        println!(
            "  git init and initial commit: {}",
            if args.no_git { "no" } else { "yes" }
        );
        return ExitCode::SUCCESS;
    }
    if let Err(err) = scaffold.create(&target, with_defaults) {
        return super::config_error(err);
    }
    if !args.no_git {
        seeds::init_git(&target, with_defaults);
    }
    println!(
        "Created private skill registry at {}",
        display_path(&target)
    );
    println!("Next: cd there, add skills/<name>/SKILL.md, then run `my-skills install --dry-run`.");
    ExitCode::SUCCESS
}

pub fn skills(args: &SkillsArgs) -> ExitCode {
    let result = (|| {
        let (manifest, resolution) = load_resolved(false)?;
        let state = State::load(None)?;
        let host = args.host.as_deref();
        if let Some(host) = host.filter(|host| !manifest.targets.contains_key(*host)) {
            return Err(Error::Manifest(format!("unknown host: {host}")));
        }
        let hosts = match host {
            Some(host) => vec![host.to_owned()],
            None => root::resolve_hosts(&manifest, None)?,
        };
        let enabled = if args.disabled {
            Some(false)
        } else {
            args.enabled.then_some(true)
        };
        let rows = catalog::rows(&manifest, &state, host, enabled, &hosts, args.with_status)?;
        Ok(if args.json {
            json_with_registry(rows, &resolution)
        } else {
            rows.table(&hosts)
        })
    })();
    match result {
        Ok(output) => {
            println!("{output}");
            ExitCode::SUCCESS
        }
        Err(err) => super::config_error(err),
    }
}

pub fn share(args: &ShareArgs) -> ExitCode {
    let preview = args.plan || args.dry_run;
    let write = !preview && args.skill.is_some() && args.enable != args.disable;
    let (manifest, resolution) = match load_resolved(write) {
        Ok(loaded) => loaded,
        Err(err) => return super::config_error(err),
    };
    if preview {
        return match share::plan_share(&manifest, &args.from_host) {
            Ok(plan) => {
                let output = if args.json {
                    json_with_registry(plan, &resolution)
                } else {
                    plan.table()
                };
                println!("{output}");
                if args.dry_run && !args.json {
                    super::print_would_cache_root(&resolution);
                }
                ExitCode::SUCCESS
            }
            Err(err) => super::config_error(err),
        };
    }
    let Some(skill) = args.skill.as_deref() else {
        return super::config_error("share requires a skill name unless --plan is used");
    };
    if args.enable == args.disable {
        return super::config_error("share requires exactly one of --enable or --disable");
    }

    let mut state = match State::load(None) {
        Ok(state) => state,
        Err(err) => return super::config_error(err),
    };
    if let Some(record) = state
        .get(skill, &args.from_host)
        .filter(|record| !record.belongs_to(&manifest.skills_dir()))
    {
        println!("[BLOCKED] {skill}: {}", record.foreign_reason());
        return ExitCode::from(1);
    }
    match share::share(
        &manifest,
        &mut state,
        &args.from_host,
        skill,
        args.enable,
        args.force,
        args.skip_audit,
    ) {
        Ok(result) => {
            if args.force {
                println!("WARN: force overwrite allowed for existing canonical skill");
            }
            if args.skip_audit {
                println!("WARN: audit skipped by explicit --skip-audit");
            }
            let name = result.name;
            println!(
                "shared: {name} -> {} ({})",
                display_path(&result.canonical),
                enabled_label(args.enable)
            );
            println!("adopted: {name} -> {}", args.from_host);
            print_hint(&name, args.enable);
            ExitCode::SUCCESS
        }
        Err(Failure::Config(err)) => super::config_error(err),
        Err(Failure::Link(path)) => {
            println!("[BLOCKED] {skill}: share source contains symlink");
            println!("  symlink: {}", display_path(&path));
            ExitCode::from(1)
        }
        Err(Failure::Validation(errors)) => {
            println!("[BLOCKED] {}", errors.join("; "));
            ExitCode::from(1)
        }
        Err(Failure::Blocked(message)) => {
            println!("[BLOCKED] {message}");
            ExitCode::from(1)
        }
        Err(Failure::Audit(gate)) => {
            println!("{}", format_gate(&gate, "AUDIT BLOCKED"));
            ExitCode::from(1)
        }
    }
}

pub fn enable(args: &SkillNameArgs) -> ExitCode {
    set_enabled(&args.skill, true, args.dry_run)
}

pub fn disable(args: &SkillNameArgs) -> ExitCode {
    set_enabled(&args.skill, false, args.dry_run)
}

fn set_enabled(name: &str, enabled: bool, dry_run: bool) -> ExitCode {
    if dry_run {
        let (manifest, resolution) = match load_resolved(false) {
            Ok(loaded) => loaded,
            Err(err) => return super::config_error(err),
        };
        let Some(skill) = manifest.skills.get(name) else {
            return super::config_error(format!("unknown skill: {name}"));
        };
        println!(
            "Dry run — would mark {name} {} (currently {}); nothing written",
            enabled_label(enabled),
            enabled_label(skill.enabled)
        );
        super::print_would_cache_root(&resolution);
        return ExitCode::SUCCESS;
    }
    let result = load_resolved(true).and_then(|(manifest, _)| {
        let path = manifest.root.join(MANIFEST_FILE);
        share::ensure_outside_hosts(&path, share::target_paths(&manifest))?;
        edit::set_skill_enabled(&path, name, enabled)
    });
    if let Err(err) = result {
        return super::config_error(err);
    }
    println!("{}: {name}", enabled_label(enabled));
    ExitCode::SUCCESS
}

pub fn import(args: &ImportArgs) -> ExitCode {
    let (manifest, resolution) = match load_resolved(!args.dry_run) {
        Ok(loaded) => loaded,
        Err(err) => return super::config_error(err),
    };
    let source = seeds::expand_user(&args.source);
    match share::import(
        &manifest,
        &source,
        args.enable,
        args.force,
        args.skip_audit,
        args.dry_run,
        || {
            println!("WARN: audit skipped by explicit --skip-audit");
        },
    ) {
        Ok(result) => {
            let name = result.name;
            let canonical = display_path(&result.canonical);
            if args.dry_run {
                println!("Dry run — {name} passed validation and audit; nothing written");
                if result.identical {
                    println!("  canonical already matches: {canonical}");
                } else if result.overwritten {
                    println!("  would overwrite (--force): {canonical}");
                } else {
                    println!("  would import to: {canonical}");
                }
                println!("  would register as: {}", enabled_label(result.enabled));
                super::print_would_cache_root(&resolution);
                return ExitCode::SUCCESS;
            }
            if result.identical {
                println!("up to date: {name} already matches the canonical skill");
            } else {
                if result.overwritten {
                    println!(
                        "WARN: force overwrite allowed for existing canonical skill: {canonical}"
                    );
                }
                println!("imported: {name} -> {canonical}");
            }
            println!("registered: {name} ({})", enabled_label(result.enabled));
            if !result.identical {
                print_hint(&name, args.enable);
            }
            ExitCode::SUCCESS
        }
        Err(Failure::Config(err)) => super::config_error(err),
        Err(Failure::Link(path)) => {
            println!(
                "[BLOCKED] {}: import source contains symlink",
                share::basename(&source)
            );
            println!("  symlink: {}", display_path(&path));
            eprintln!("\nNothing was imported (fix the source first).");
            ExitCode::from(1)
        }
        Err(Failure::Validation(errors)) => {
            println!("[BLOCKED] {}: validation failed", share::basename(&source));
            for error in errors {
                println!("  error: {error}");
            }
            eprintln!("\nNothing was imported (fix the source first).");
            ExitCode::from(1)
        }
        Err(Failure::Audit(gate)) => {
            println!("{}", format_gate(&gate, "AUDIT BLOCKED"));
            eprintln!("\nNothing was imported (audit blocked).");
            ExitCode::from(1)
        }
        Err(Failure::Blocked(message)) => {
            println!("[BLOCKED] {message}");
            if message.contains("a different canonical skill already exists at") {
                eprintln!("Re-run with --force to overwrite it. Nothing was changed.");
            }
            ExitCode::from(1)
        }
    }
}

fn json_with_registry(output: impl Serialize, resolution: &root::RootResolution) -> String {
    #[derive(Serialize)]
    struct WithRegistry<T> {
        #[serde(flatten)]
        output: T,
        registry: root::RegistryMeta,
    }

    json_output(&WithRegistry {
        output,
        registry: resolution.meta(),
    })
}

fn load_resolved(write_cache: bool) -> Result<(Manifest, root::RootResolution)> {
    root::load_manifest_resolved(&CliOverrides::default(), write_cache)
}

fn enabled_label(enabled: bool) -> &'static str {
    if enabled { "enabled" } else { "disabled" }
}

fn print_hint(name: &str, enabled: bool) {
    if enabled {
        println!("Next: `my-skills sync {name} --host <host>`.");
    } else {
        println!("Next: `my-skills enable {name}`, then `my-skills sync {name} --host <host>`.");
    }
}
