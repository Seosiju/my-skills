//! init-registry, skills, share, enable, disable, import. Stream H.

use std::process::ExitCode;

use crate::audit::format_gate;
use crate::catalog::{self, display_path, json_output};
use crate::cli::*;
use crate::manifest::{CliOverrides, MANIFEST_FILE, Manifest};
use crate::manifest_edit;
use crate::seeds::{self, Scaffold};
use crate::sharing::{self, Failure};
use crate::state::State;
use crate::{Error, Result, runtime};

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
        let manifest = load()?;
        let state = State::load(None)?;
        let host = args.host.as_deref();
        if let Some(host) = host.filter(|host| !manifest.targets.contains_key(*host)) {
            return Err(Error::Manifest(format!("unknown host: {host}")));
        }
        let hosts = match host {
            Some(host) => vec![host.to_owned()],
            None => runtime::resolve_hosts(&manifest, None)?,
        };
        let enabled = if args.disabled {
            Some(false)
        } else {
            args.enabled.then_some(true)
        };
        let rows = catalog::rows(&manifest, &state, host, enabled, &hosts, args.with_status)?;
        Ok(if args.json {
            json_output(&rows)
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
    let manifest = match load() {
        Ok(manifest) => manifest,
        Err(err) => return super::config_error(err),
    };
    if args.plan || args.dry_run {
        return match sharing::plan_share(&manifest, &args.from_host) {
            Ok(plan) => {
                let output = if args.json {
                    json_output(&plan)
                } else {
                    plan.table()
                };
                println!("{output}");
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
    match sharing::share(
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
        let manifest = match load() {
            Ok(manifest) => manifest,
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
        return ExitCode::SUCCESS;
    }
    let result = load().and_then(|manifest| {
        let path = manifest.root.join(MANIFEST_FILE);
        sharing::ensure_outside_hosts(&path, sharing::target_paths(&manifest))?;
        manifest_edit::set_skill_enabled(&path, name, enabled)
    });
    if let Err(err) = result {
        return super::config_error(err);
    }
    println!("{}: {name}", enabled_label(enabled));
    ExitCode::SUCCESS
}

pub fn import(args: &ImportArgs) -> ExitCode {
    let manifest = match load() {
        Ok(manifest) => manifest,
        Err(err) => return super::config_error(err),
    };
    let source = seeds::expand_user(&args.source);
    match sharing::import(
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
                sharing::basename(&source)
            );
            println!("  symlink: {}", display_path(&path));
            eprintln!("\nNothing was imported (fix the source first).");
            ExitCode::from(1)
        }
        Err(Failure::Validation(errors)) => {
            println!(
                "[BLOCKED] {}: validation failed",
                sharing::basename(&source)
            );
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

fn load() -> Result<Manifest> {
    runtime::load_manifest_from_cwd(&CliOverrides::default())
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
