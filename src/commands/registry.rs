//! init-registry, skills, share, enable, disable, import. Stream H.

use std::io::Write as _;
use std::path::Path;
use std::process::ExitCode;

use serde::Serialize;

use crate::check::validate::validate_name;
use crate::cli::*;
use crate::drive::state::State;
use crate::registry::catalog::{self, display_path, json_output};
use crate::registry::edit;
use crate::registry::manifest::{self, CliOverrides, MANIFEST_FILE, Manifest};
use crate::registry::seeds::{self, Scaffold};
use crate::registry::share::{self, Failure};
use crate::{Error, Result, registry::root};

pub fn init_registry(args: &InitRegistryArgs) -> ExitCode {
    let cache = root::CacheSnapshot::observe();
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
        println!(
            "  git hooks: {}",
            if args.no_git || args.no_hooks {
                "no"
            } else {
                "yes"
            }
        );
        return ExitCode::SUCCESS;
    }
    if let Err(err) = scaffold.create(&target, with_defaults) {
        return super::config_error(err);
    }
    let created = match root::validate_repo_root(&target) {
        Ok(path) => path,
        Err(err) => return super::config_error(err),
    };
    cache.remember(&created);
    if !args.no_git {
        seeds::init_git(&target, with_defaults);
        if !args.no_hooks
            && created.join(".git").exists()
            && let Ok(git) = crate::git::hooks::Git::discover(&created)
        {
            super::hooks::fill_for_setup(&git);
        }
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
        let mut names = manifest::select_names(&manifest, &args.skills, &[])?;
        if !args.groups.is_empty() {
            let group_names = manifest::select_names(&manifest, &[], &args.groups)?;
            if args.skills.is_empty() {
                names = group_names;
            } else {
                names.retain(|name| group_names.binary_search(name).is_ok());
            }
        }
        let selected =
            (!args.skills.is_empty() || !args.groups.is_empty()).then_some(names.as_slice());
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
        let rows = catalog::rows(&manifest, &state, host, enabled, &hosts, selected, false)?;
        Ok(if args.json {
            if args.brief {
                json_with_registry(rows.brief(), &resolution)
            } else {
                json_with_registry(rows, &resolution)
            }
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

pub fn enable(args: &SkillNameArgs) -> ExitCode {
    set_enabled(args, true)
}

pub fn disable(args: &SkillNameArgs) -> ExitCode {
    set_enabled(args, false)
}

fn set_enabled(args: &SkillNameArgs, enabled: bool) -> ExitCode {
    if args.skills.is_empty() && args.groups.is_empty() {
        let command = if enabled { "enable" } else { "disable" };
        return super::config_error(format!("{command} requires a skill name"));
    }
    let result = (|| {
        let (manifest, resolution) = if args.dry_run {
            load_resolved(false)?
        } else {
            let resolution = root::resolve_root(None, false)?;
            if let Some(refusal) = resolution.write_refusal() {
                return Err(Error::Manifest(refusal));
            }
            let manifest = manifest::load_manifest(&resolution.root, &CliOverrides::default())?;
            (manifest, resolution)
        };
        let names = match manifest::select_names(&manifest, &args.skills, &args.groups) {
            Ok(names) => names,
            Err(_) if args.skills.len() == 1 && args.groups.is_empty() => {
                return Err(Error::Manifest(format!(
                    "unknown skill: {}",
                    args.skills[0]
                )));
            }
            Err(e) => return Err(e),
        };
        let label = enabled_label(enabled);
        if args.dry_run {
            for name in &names {
                let current = manifest.skills[name].enabled;
                if current == enabled {
                    println!("Dry run — {name} already {label}; nothing written");
                } else {
                    println!(
                        "Dry run — would mark {name} {label} (currently {}); nothing written",
                        enabled_label(current)
                    );
                }
            }
            super::print_would_cache_root(&resolution);
            return Ok(());
        }
        let path = manifest.root.join(MANIFEST_FILE);
        share::ensure_outside_hosts(&path, share::target_paths(&manifest))?;
        let edit = edit::prepare_enabled(&path, &names, enabled)?;
        resolution.recheck_write()?;
        resolution.remember();
        edit.commit()?;
        for name in &names {
            println!("{label}: {name}");
        }
        Ok(())
    })();
    if let Err(err) = result {
        return super::config_error(err);
    }
    ExitCode::SUCCESS
}

pub fn import(args: &ImportArgs) -> ExitCode {
    if let Some(from_host) = args.from_host.as_deref() {
        if args.plan && args.target.is_some() {
            return super::config_error("--plan cannot be combined with a skill name");
        }
        if args.plan || (args.dry_run && args.target.is_none()) {
            let (manifest, resolution) = match load_resolved(false) {
                Ok(loaded) => loaded,
                Err(err) => return super::config_error(err),
            };
            return match share::plan_share(&manifest, from_host) {
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
        let Some(skill) = args.target.as_deref() else {
            return super::config_error(
                "import --from requires a skill name unless --plan is used",
            );
        };
        if args.enable == args.disable {
            return super::config_error("import --from requires either --enable or --disable");
        }
        let resolution = match root::resolve_root(None, false) {
            Ok(resolution) => resolution,
            Err(err) => return super::config_error(err),
        };
        if let Some(refusal) = resolution.write_refusal() {
            return super::config_error(refusal);
        }
        return match share::share(&resolution, from_host, skill, args.enable, args.force) {
            Ok(result) => {
                if args.force {
                    println!("WARN: force overwrite allowed for existing canonical skill");
                }
                let name = result.name;
                println!(
                    "imported: {name} -> {} ({})",
                    display_path(&result.canonical),
                    enabled_label(args.enable)
                );
                println!("adopted: {name} -> {from_host}");
                print_hint(&name, args.enable);
                if let Some(warning) = result.cleanup_warning {
                    eprintln!("warning: {warning}");
                }
                ExitCode::SUCCESS
            }
            Err(Failure::Config(err)) => super::config_error(err),
            Err(Failure::Link(path)) => {
                println!("[BLOCKED] {skill}: import source contains symlink");
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
        };
    }

    if args.plan {
        return super::config_error("--plan requires --from <host>");
    }
    let Some(raw_source) = args.target.as_deref() else {
        return super::config_error("import requires a source path unless --from is used");
    };
    let (manifest, resolution) = match load_resolved(!args.dry_run) {
        Ok(loaded) => loaded,
        Err(err) => return super::config_error(err),
    };
    let source = seeds::expand_user(raw_source);
    match share::import(&manifest, &source, args.enable, args.force, args.dry_run) {
        Ok(result) => {
            let name = result.name;
            let canonical = display_path(&result.canonical);
            for warning in &result.warnings {
                println!("  warn: {warning}");
            }
            if args.dry_run {
                println!("Dry run — {name} passed validation; nothing written");
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
            if let Some(warning) = result.cleanup_warning {
                eprintln!("warning: {warning}");
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

// ---------------------------------------------------------------- new (W16 S2)

const NEW_DESCRIPTION_PLACEHOLDER: &str =
    "TODO: replace this with what the skill does and when an agent should use it.";
/// Every `--description` validation message starts with this, so the JSON error
/// maps to `usage`.
pub const NEW_DESCRIPTION_ERROR: &str = "invalid --description: ";

#[derive(Serialize)]
struct NewOutput {
    skill: String,
    enabled: bool,
    group: Option<String>,
    hosts: Vec<String>,
    canonical_dir: String,
    canonical_path: String,
    created: Vec<String>,
    next: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    warnings: Vec<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    dry_run: bool,
}

/// `new <name>`: create `skills/<name>/SKILL.md` and register it disabled.
pub fn new_skill(args: &NewArgs) -> ExitCode {
    match create_new_skill(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => super::config_error(err),
    }
}

fn create_new_skill(args: &NewArgs) -> Result<()> {
    let name = args.name.as_str();
    if let Some(error) = validate_name(name).first() {
        return Err(Error::Manifest(format!(
            "invalid skill name '{name}': {error}"
        )));
    }
    if let Some(group) = &args.group
        && let Some(error) = validate_name(group).first()
    {
        return Err(Error::Manifest(format!(
            "invalid group name '{group}': {error}"
        )));
    }
    let description = match args.description.as_deref().map(str::trim) {
        None => NEW_DESCRIPTION_PLACEHOLDER,
        Some("") => {
            return Err(Error::Manifest(format!(
                "{NEW_DESCRIPTION_ERROR}must not be empty"
            )));
        }
        Some(text) => text,
    };
    let resolution = root::resolve_root(None, false)?;
    if let Some(refusal) = resolution.write_refusal() {
        return Err(Error::Manifest(refusal));
    }
    let manifest_path = resolution.root.join(MANIFEST_FILE);
    // One edit lock covers the checks, the canonical write, the registration,
    // and any rollback. `new` never touches state.
    let _lock = if args.dry_run {
        edit::EditLock::check_available(&manifest_path)?;
        None
    } else {
        Some(edit::EditLock::acquire(&manifest_path)?)
    };
    if !args.dry_run {
        // `new` takes no state lock, so recheck the cache it observed.
        resolution.recheck_write()?;
    }
    let manifest = manifest::load_manifest(&resolution.root, &CliOverrides::default())?;
    let canonical_dir = manifest.skill_dir(name);
    let canonical_path = canonical_dir.join("SKILL.md");

    // Any kind of entry counts, including a file or a broken link.
    match std::fs::symlink_metadata(&canonical_dir) {
        Ok(_) => {
            return Err(Error::Manifest(format!(
                "skill already exists: {} is already there",
                display_path(&canonical_dir)
            )));
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(Error::io(&canonical_dir, err)),
    }
    if manifest.skills.contains_key(name) {
        return Err(Error::Manifest(format!(
            "skill already exists: {name} is already registered in {MANIFEST_FILE} or {}",
            manifest::LOCAL_MANIFEST_FILE
        )));
    }
    let mut hosts: Vec<String> = Vec::new();
    for host in &args.hosts {
        if !manifest.targets.contains_key(host) {
            return Err(Error::Manifest(format!("unknown host: {host}")));
        }
        if !hosts.contains(host) {
            hosts.push(host.clone());
        }
    }
    share::ensure_outside_hosts(&manifest_path, share::target_paths(&manifest))?;
    share::ensure_outside_hosts(&canonical_dir, share::target_paths(&manifest))?;
    let warnings = crate::drive::hosts::ALL
        .into_iter()
        .filter(|host| {
            if hosts.is_empty() {
                manifest
                    .targets
                    .get(host.name)
                    .is_some_and(|target| target.enabled)
            } else {
                hosts.iter().any(|name| name == host.name)
            }
        })
        .filter_map(|host| crate::check::validate::description_warning(host, description, None))
        .collect();

    let canonical_dir_text = display_path(&canonical_dir);
    let canonical_path_text = display_path(&canonical_path);
    let output = NewOutput {
        skill: name.to_owned(),
        enabled: false,
        group: args.group.clone(),
        hosts: hosts.clone(),
        created: vec![canonical_dir_text.clone(), canonical_path_text.clone()],
        next: vec![
            format!("Edit {canonical_path_text}"),
            format!("my-skills enable {name}"),
            format!("my-skills sync {name} --dry-run --json, then commit"),
        ],
        canonical_dir: canonical_dir_text,
        canonical_path: canonical_path_text,
        dry_run: args.dry_run,
        warnings,
    };

    if !args.dry_run {
        materialize_skill(
            &canonical_dir,
            &canonical_path,
            &new_skill_text(name, description),
            || {
                edit::prepare_new_skill(&manifest_path, name, args.group.as_deref(), &hosts)?
                    .commit()
            },
        )?;
        // Conditional on the cache observed at resolution (the other writers'
        // rule): a newer `set-root` is never overwritten.
        resolution.remember();
    }

    if args.json {
        println!("{}", json_with_registry(&output, &resolution));
        return Ok(());
    }
    let verb = if args.dry_run {
        println!("Dry run — nothing written");
        "would create"
    } else {
        "created"
    };
    for path in &output.created {
        println!("{verb}: {path}");
    }
    println!(
        "{} {name} in {MANIFEST_FILE} (disabled{}{})",
        if args.dry_run {
            "would register"
        } else {
            "registered"
        },
        args.group
            .as_deref()
            .map(|group| format!(", group {group}"))
            .unwrap_or_default(),
        if hosts.is_empty() {
            String::new()
        } else {
            format!(", hosts {}", hosts.join(", "))
        }
    );
    for warning in &output.warnings {
        println!("  warn: {warning}");
    }
    println!("Next:");
    for (index, step) in output.next.iter().enumerate() {
        println!("  {}. {step}", index + 1);
    }
    if args.dry_run {
        super::print_would_cache_root(&resolution);
    }
    Ok(())
}

fn new_skill_text(name: &str, description: &str) -> String {
    // Quoting also keeps names such as "off" and "0123" as strings.
    let name_yaml = serde_json::to_string(name).expect("a string is serializable");
    let description = serde_json::to_string(description)
        .expect("a string is serializable")
        .replace('\u{85}', "\\u0085")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029");
    format!(
        "---\nname: {name_yaml}\ndescription: {description}\n---\n\n\
         # {name}\n\n\
         ## When to use\n\n\
         Describe the requests and situations that should trigger this skill.\n\n\
         ## Steps\n\n\
         1. Describe the first step.\n\
         2. Describe what to verify before finishing.\n\n\
         ## Notes\n\n\
         Keep this file short; put long references in `references/`.\n"
    )
}

/// Create the directory and SKILL.md, then run `register`. Any failure removes
/// only what this call created (SKILL.md and its directory, never `skills/`
/// and never a file someone else put there). A rollback that cannot finish is
/// reported with the original error.
fn materialize_skill(
    dir: &Path,
    skill_md: &Path,
    text: &str,
    register: impl FnOnce() -> Result<()>,
) -> Result<()> {
    if let Some(parent) = dir.parent() {
        std::fs::create_dir_all(parent).map_err(|err| Error::io(parent, err))?;
    }
    // Fails when anything appeared at `dir` since the checks.
    std::fs::create_dir(dir).map_err(|err| Error::io(dir, err))?;
    let mut created_file = false;
    let write = || -> Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(skill_md)
            .map_err(|err| Error::io(skill_md, err))?;
        created_file = true;
        file.write_all(edit::native_text(text).as_bytes())
            .map_err(|err| Error::io(skill_md, err))?;
        drop(file);
        register()
    };
    let Err(error) = write() else {
        return Ok(());
    };
    let mut problems = Vec::new();
    if created_file && let Err(err) = std::fs::remove_file(skill_md) {
        problems.push(format!("{}: {err}", display_path(skill_md)));
    }
    // A directory that now holds someone else's files is theirs to keep.
    let occupied = std::fs::read_dir(dir).is_ok_and(|mut entries| entries.next().is_some());
    if (created_file || !occupied)
        && let Err(err) = std::fs::remove_dir(dir)
    {
        problems.push(format!("{}: {err}", display_path(dir)));
    }
    if problems.is_empty() {
        Err(error)
    } else {
        Err(Error::Manifest(format!(
            "{error}; rollback failed, remove by hand: {}",
            problems.join("; ")
        )))
    }
}

#[cfg(test)]
mod new_tests {
    use super::*;

    #[test]
    fn failed_registration_removes_only_the_new_skill_directory() {
        let temp = tempfile::tempdir().unwrap();
        let skills = temp.path().join("skills");
        std::fs::create_dir_all(skills.join("other")).unwrap();
        std::fs::write(skills.join("other/SKILL.md"), "keep").unwrap();
        let dir = skills.join("draft");
        let result = materialize_skill(&dir, &dir.join("SKILL.md"), "text", || {
            assert!(dir.join("SKILL.md").is_file());
            Err(Error::Manifest("registration failed".into()))
        });
        assert!(result.is_err());
        assert!(!dir.exists());
        assert_eq!(
            std::fs::read_to_string(skills.join("other/SKILL.md")).unwrap(),
            "keep"
        );
    }

    #[test]
    fn an_existing_entry_is_never_removed() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("skills/draft");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("SKILL.md"), "mine").unwrap();
        let result = materialize_skill(&dir, &dir.join("SKILL.md"), "text", || Ok(()));
        assert!(result.is_err());
        assert_eq!(
            std::fs::read_to_string(dir.join("SKILL.md")).unwrap(),
            "mine"
        );
    }
}

#[cfg(test)]
mod rollback_tests {
    use super::*;

    #[test]
    fn a_skill_file_someone_else_created_is_never_removed() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("skills/draft");
        // The file already exists, so `create_new` fails after our directory
        // was made.
        let theirs = temp.path().join("theirs.md");
        std::fs::write(&theirs, "theirs").unwrap();
        let result = materialize_skill(&dir, &theirs, "text", || Ok(()));
        assert!(result.is_err());
        assert_eq!(std::fs::read_to_string(&theirs).unwrap(), "theirs");
        assert!(!dir.exists(), "the empty directory we made is removed");
    }

    #[test]
    fn a_directory_holding_foreign_files_is_kept_and_the_failure_is_reported() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("skills/draft");
        let skill_md = dir.join("SKILL.md");
        let result = materialize_skill(&dir, &skill_md, "text", || {
            std::fs::write(dir.join("theirs.txt"), "theirs").unwrap();
            Err(Error::Manifest("registration failed".into()))
        });
        let message = result.unwrap_err().to_string();
        assert!(message.contains("registration failed"), "{message}");
        assert!(message.contains("rollback failed"), "{message}");
        assert!(!skill_md.exists(), "our own file is removed");
        assert_eq!(
            std::fs::read_to_string(dir.join("theirs.txt")).unwrap(),
            "theirs"
        );
    }
}
