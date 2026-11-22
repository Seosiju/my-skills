//! Environment, registry, and connection diagnostics.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde::Serialize;

use crate::cli::DoctorArgs;
use crate::hooks::{self, Git, HookStatus};
use crate::link::{self, Created, LinkKind};
use crate::manifest::{self, InstallMode, Manifest};
use crate::runtime::{self, RootSource};
use crate::state::{self, State};
use crate::{hosts, paths, plan, self_update, validate};

const PYTHON: &str = "not required (Rust binary)";

#[derive(Serialize)]
struct Report {
    version: &'static str,
    update: String,
    os: String,
    shell: String,
    python: &'static str,
    registry: Registry,
    skills: Option<Skills>,
    state_path: PathBuf,
    data_root: PathBuf,
    hosts: Vec<HostStatus>,
    manifest: ManifestHealth,
    link_capability: LinkCapability,
    registry_git: Option<GitStatus>,
    hooks: Option<HookStatus>,
    hooks_error: Option<String>,
    connections: Connections,
    full_validation: Option<FullValidation>,
}

#[derive(Default, Serialize)]
struct Registry {
    path: Option<PathBuf>,
    source: Option<&'static str>,
    error: Option<String>,
    warning: Option<String>,
}

#[derive(Serialize)]
struct Skills {
    registered: usize,
    enabled: usize,
}

#[derive(Serialize)]
struct HostStatus {
    name: &'static str,
    display_name: &'static str,
    executable: Option<&'static str>,
    enabled: bool,
    writable: bool,
    path: PathBuf,
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum ManifestHealth {
    Valid,
    NotChecked,
    Invalid { error: String },
}

#[derive(Serialize)]
struct LinkCapability {
    kind: &'static str,
    reason: Option<String>,
}

#[derive(Default, Serialize)]
struct GitStatus {
    branch: Option<String>,
    default_branch: Option<String>,
    rebase_in_progress: bool,
    merge_in_progress: bool,
    uncommitted: Option<usize>,
    conflicts: Option<usize>,
    upstream: Option<String>,
    ahead: Option<usize>,
    behind: Option<usize>,
    warnings: Vec<String>,
}

#[derive(Default, Serialize)]
struct Connections {
    link: usize,
    copy: usize,
    fallback: usize,
    statuses: BTreeMap<&'static str, usize>,
    warnings: Vec<String>,
}

#[derive(Serialize)]
struct FullValidation {
    checked: usize,
    failures: Vec<SkillFailure>,
}

#[derive(Serialize)]
struct SkillFailure {
    skill: String,
    errors: Vec<String>,
}

pub fn run(args: &DoctorArgs) -> ExitCode {
    let report = Report::collect(args.no_update_check);
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).expect("doctor report is serializable")
        );
    } else {
        report.print();
    }
    if matches!(report.manifest, ManifestHealth::Invalid { .. }) {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

impl Report {
    fn collect(no_update_check: bool) -> Self {
        let update = self_update::update_status_line(no_update_check);
        let update = if no_update_check {
            "skipped".to_owned()
        } else {
            update
        };
        let (registry, manifest_health, manifest) = registry();
        let skills = manifest.as_ref().map(|manifest| Skills {
            registered: manifest.skills.len(),
            enabled: manifest
                .skills
                .values()
                .filter(|skill| skill.enabled)
                .count(),
        });
        let (registry_git, hook_status, hooks_error) = git_diagnostics(registry.path.as_deref());
        let hosts = hosts::ALL
            .map(|host| HostStatus::collect(host, manifest.as_ref()))
            .into();
        Self {
            version: env!("CARGO_PKG_VERSION"),
            update,
            os: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
            shell: std::env::var_os("SHELL").map_or_else(
                || "unknown".into(),
                |shell| shell.to_string_lossy().into_owned(),
            ),
            python: PYTHON,
            registry,
            skills,
            state_path: state::default_state_path(),
            data_root: paths::data_root(),
            hosts,
            manifest: manifest_health,
            link_capability: LinkCapability::probe(),
            registry_git,
            hooks: hook_status,
            hooks_error,
            connections: Connections::collect(manifest.as_ref()),
            full_validation: manifest.as_ref().map(FullValidation::collect),
        }
    }

    fn print(&self) {
        println!("my-skills {}", self.version);
        println!("Update:  {}", self.update);
        println!("OS:     {}", self.os);
        println!("Shell:  {}", self.shell);
        println!("Python: {}", self.python);
        println!();
        if let (Some(path), Some(source)) = (&self.registry.path, self.registry.source) {
            println!("Registry: {} (source: {source})", path.display());
        } else if let Some(error) = &self.registry.error {
            println!("Registry: INVALID ({error})");
        } else {
            println!(
                "Registry: not configured (run 'my-skills init-registry' or 'my-skills set-root')"
            );
        }
        if let Some(warning) = &self.registry.warning {
            println!("warning: {warning}");
        }
        if let Some(skills) = &self.skills {
            println!(
                "Skills:   {} registered, {} enabled",
                skills.registered, skills.enabled
            );
        }
        println!("State:    {}", self.state_path.display());
        println!("Data:     {}", self.data_root.display());
        println!("\nHosts:");
        for host in &self.hosts {
            let detected = host
                .executable
                .map_or_else(|| "not found".into(), |exe| format!("found ({exe})"));
            println!(
                "  {:12} exe={detected:18} enabled={:5} writable={:5} path={}",
                host.display_name,
                python_bool(host.enabled),
                python_bool(host.writable),
                host.path.display()
            );
        }
        match &self.manifest {
            ManifestHealth::Valid => println!("\nManifest: valid"),
            ManifestHealth::NotChecked => println!("\nManifest: not checked"),
            ManifestHealth::Invalid { error } => println!("\nManifest: INVALID ({error})"),
        }

        match &self.link_capability.reason {
            Some(reason) => println!("\nLink capability: {}: {reason}", self.link_capability.kind),
            None => println!("\nLink capability: {}", self.link_capability.kind),
        }
        if let Some(status) = &self.registry_git {
            status.print();
        } else {
            println!("\nRegistry git: not checked");
        }
        println!("\nHooks:");
        if let Some(hooks) = &self.hooks {
            hooks.print();
        } else if let Some(error) = &self.hooks_error {
            println!("  warning: {error}");
        } else {
            println!("  not checked");
        }
        self.connections.print();
        if let Some(validation) = &self.full_validation {
            validation.print();
        } else {
            println!("\nFull validation: not checked");
        }
    }
}

fn registry() -> (Registry, ManifestHealth, Option<Manifest>) {
    let resolution = match runtime::resolve_root(None, false) {
        Ok(resolution) => resolution,
        Err(error) if error.to_string().starts_with("my-skills.toml not found") => {
            return (Registry::default(), ManifestHealth::NotChecked, None);
        }
        Err(error) => {
            let error = error.to_string();
            return (
                Registry {
                    error: Some(error.clone()),
                    ..Default::default()
                },
                ManifestHealth::Invalid { error },
                None,
            );
        }
    };
    let warning = resolution
        .cached
        .filter(|active| resolution.source == RootSource::Cwd && *active != resolution.root)
        .map(|active| {
            format!(
                "this directory is not the active registry (active: {})",
                active.display()
            )
        });
    let manifest = manifest::load_manifest(&resolution.root, &Default::default());
    let registry = Registry {
        path: Some(resolution.root),
        source: Some(resolution.source.label()),
        warning,
        ..Default::default()
    };
    match manifest {
        Ok(manifest) => (registry, ManifestHealth::Valid, Some(manifest)),
        Err(error) => (
            registry,
            ManifestHealth::Invalid {
                error: error.to_string(),
            },
            None,
        ),
    }
}

impl HostStatus {
    fn collect(host: &hosts::HostConfig, manifest: Option<&Manifest>) -> Self {
        let target = manifest.and_then(|manifest| manifest.targets.get(host.name));
        let path = target.map_or_else(
            || manifest::expand_path(host.default_user_path),
            |target| target.path.clone(),
        );
        Self {
            name: host.name,
            display_name: host.display_name,
            executable: host
                .detect_commands
                .iter()
                .copied()
                .find(|name| hooks::command_on_path(name)),
            enabled: target.map_or(host.default_enabled, |target| target.enabled),
            writable: writable(&path),
            path,
        }
    }
}

fn python_bool(value: bool) -> &'static str {
    if value { "True" } else { "False" }
}

fn writable(path: &Path) -> bool {
    path.ancestors()
        .find(|ancestor| ancestor.exists())
        .is_some_and(|ancestor| {
            #[cfg(unix)]
            return hooks::unix_test("-w", ancestor);
            #[cfg(not(unix))]
            return fs::metadata(ancestor)
                .is_ok_and(|metadata| metadata.is_dir() || !metadata.permissions().readonly());
        })
}

impl LinkCapability {
    fn probe() -> Self {
        match probe_link() {
            Ok(Created::Link(LinkKind::Symlink)) => Self {
                kind: "symlink",
                reason: None,
            },
            Ok(Created::Link(LinkKind::Junction)) => Self {
                kind: "junction",
                reason: None,
            },
            Ok(Created::NeedsCopy { reason }) => Self {
                kind: "copy fallback",
                reason: Some(reason),
            },
            Err(error) => Self {
                kind: "copy fallback",
                reason: Some(error.to_string()),
            },
        }
    }
}

fn probe_link() -> anyhow::Result<Created> {
    let tmp = tempfile::tempdir()?;
    let source = fs::canonicalize(tmp.path())?.join("source");
    let dest = tmp.path().join("link");
    fs::create_dir(&source)?;
    let created = link::create_link(&dest, &source)?;
    if matches!(created, Created::Link(_)) {
        link::remove_link(&dest)?;
    }
    Ok(created)
}

fn git_diagnostics(root: Option<&Path>) -> (Option<GitStatus>, Option<HookStatus>, Option<String>) {
    let Some(root) = root else {
        return (None, None, None);
    };
    let git = match Git::discover(root) {
        Ok(git) => git,
        Err(error) => {
            let status = GitStatus {
                warnings: vec![error.to_string()],
                ..Default::default()
            };
            return (Some(status), None, None);
        }
    };
    let status = GitStatus::collect(&git);
    match hooks::status(&git) {
        Ok(hooks) => (Some(status), Some(hooks), None),
        Err(error) => (Some(status), None, Some(error.to_string())),
    }
}

impl GitStatus {
    fn collect(git: &Git<'_>) -> Self {
        let default_branch = git
            .text(&["symbolic-ref", "--quiet", "refs/remotes/origin/HEAD"])
            .ok()
            .and_then(|name| name.strip_prefix("refs/remotes/origin/").map(str::to_owned))
            .unwrap_or_else(|| "main".into());
        let branch = git
            .text(&["symbolic-ref", "--quiet", "HEAD"])
            .ok()
            .and_then(|name| name.strip_prefix("refs/heads/").map(str::to_owned));
        let mut status = Self {
            branch,
            default_branch: Some(default_branch.clone()),
            ..Default::default()
        };
        match &status.branch {
            None => status.warnings.push(format!(
                "HEAD is detached (default branch: {default_branch})"
            )),
            Some(branch) if *branch != default_branch => status.warnings.push(format!(
                "current branch {branch} differs from default branch {default_branch}"
            )),
            Some(_) => {}
        }
        let rebase_merge = status.marker_exists(git, "rebase-merge");
        let rebase_apply = status.marker_exists(git, "rebase-apply");
        status.rebase_in_progress = rebase_merge || rebase_apply;
        status.merge_in_progress = status.marker_exists(git, "MERGE_HEAD");
        if status.rebase_in_progress {
            status.warnings.push("rebase in progress".into());
        }
        if status.merge_in_progress {
            status.warnings.push("merge in progress".into());
        }

        match git.run(&[
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
            "--",
        ]) {
            Ok(output) => {
                let (changes, conflicts) = count_changes(&output);
                status.uncommitted = Some(changes);
                status.conflicts = Some(conflicts);
                if changes != 0 {
                    status
                        .warnings
                        .push(format!("{changes} uncommitted changes"));
                }
                if conflicts != 0 {
                    status
                        .warnings
                        .push(format!("{conflicts} unresolved conflicts"));
                }
            }
            Err(error) => status.warnings.push(error.to_string()),
        }

        status.upstream = git
            .text(&[
                "rev-parse",
                "--abbrev-ref",
                "--symbolic-full-name",
                "@{upstream}",
            ])
            .ok();
        if let Some(upstream) = &status.upstream {
            match git
                .text(&["rev-list", "--left-right", "--count", "HEAD...@{upstream}"])
                .and_then(|counts| parse_counts(&counts))
            {
                Ok((ahead, behind)) => {
                    status.ahead = Some(ahead);
                    status.behind = Some(behind);
                    if ahead != 0 || behind != 0 {
                        status
                            .warnings
                            .push(format!("ahead {ahead} / behind {behind} ({upstream})"));
                    }
                }
                Err(error) => status.warnings.push(error.to_string()),
            }
        }
        status
    }

    fn marker_exists(&mut self, git: &Git<'_>, name: &str) -> bool {
        match git.path(name).and_then(|path| Ok(path.try_exists()?)) {
            Ok(present) => present,
            Err(error) => {
                self.warnings
                    .push(format!("could not check {name}: {error}"));
                false
            }
        }
    }

    fn print(&self) {
        println!("\nRegistry git:");
        if let Some(default) = &self.default_branch {
            println!(
                "  Branch: {} (default: {default})",
                self.branch.as_deref().unwrap_or("detached HEAD")
            );
            println!(
                "  Rebase: {} / Merge: {}",
                yes_no(self.rebase_in_progress),
                yes_no(self.merge_in_progress)
            );
            if let (Some(changes), Some(conflicts)) = (self.uncommitted, self.conflicts) {
                println!("  Uncommitted: {changes} (conflicts: {conflicts})");
            }
            match (&self.upstream, self.ahead, self.behind) {
                (Some(upstream), Some(ahead), Some(behind)) => {
                    println!("  Upstream: {upstream} (ahead {ahead} / behind {behind})");
                }
                (Some(upstream), _, _) => println!("  Upstream: {upstream} (counts unavailable)"),
                (None, _, _) => println!("  Upstream: not configured"),
            }
        }
        print_warnings(&self.warnings);
    }
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

fn count_changes(status: &[u8]) -> (usize, usize) {
    let mut entries = status
        .split(|&byte| byte == 0)
        .filter(|part| !part.is_empty());
    let mut changes = 0;
    let mut conflicts = 0;
    while let Some(entry) = entries.next() {
        changes += 1;
        if let Some(code) = entry.get(..2) {
            if matches!(code, b"DD" | b"AU" | b"UD" | b"UA" | b"DU" | b"AA" | b"UU") {
                conflicts += 1;
            }
            // In NUL-delimited porcelain format a rename/copy consumes
            // a second field containing the original path.
            if code.contains(&b'R') || code.contains(&b'C') {
                entries.next();
            }
        }
    }
    (changes, conflicts)
}

fn parse_counts(counts: &str) -> anyhow::Result<(usize, usize)> {
    let mut fields = counts.split_whitespace();
    if let (Some(ahead), Some(behind), None) = (fields.next(), fields.next(), fields.next()) {
        Ok((ahead.parse()?, behind.parse()?))
    } else {
        anyhow::bail!("unexpected Git ahead/behind counts: {counts}");
    }
}

impl Connections {
    fn collect(manifest: Option<&Manifest>) -> Self {
        let mut report = Self::default();
        let state = match State::load(None) {
            Ok(state) => state,
            Err(error) => {
                report.warnings.push(error.to_string());
                return report;
            }
        };
        for record in state.records() {
            match record.mode {
                InstallMode::Link => report.link += 1,
                InstallMode::Copy => {
                    report.copy += 1;
                    report.fallback += usize::from(record.fallback_reason.is_some());
                }
            }
        }
        if let Some(manifest) = manifest {
            for skill in manifest.skills.values() {
                for target in manifest.targets.values().filter(|target| target.enabled) {
                    match plan::status_of(manifest, skill, &target.name, &state) {
                        Ok(status) => *report.statuses.entry(status.label()).or_default() += 1,
                        Err(error) => report
                            .warnings
                            .push(format!("{} / {}: {error}", skill.name, target.name)),
                    }
                }
            }
        }
        report
    }

    fn print(&self) {
        println!(
            "\nConnections: link {} / copy {} (fallback {})",
            self.link, self.copy, self.fallback
        );
        for (status, count) in &self.statuses {
            println!("  {status}: {count}");
        }
        print_warnings(&self.warnings);
    }
}

impl FullValidation {
    fn collect(manifest: &Manifest) -> Self {
        let mut report = Self {
            checked: 0,
            failures: Vec::new(),
        };
        for skill in manifest.skills.values().filter(|skill| skill.enabled) {
            report.checked += 1;
            let validation = validate::compose_validation(&manifest.skill_dir(&skill.name));
            if !validation.ok() {
                report.failures.push(SkillFailure {
                    skill: skill.name.clone(),
                    errors: validation.errors,
                });
            }
        }
        report
    }

    fn print(&self) {
        println!(
            "\nFull validation: {} checked, {} failed",
            self.checked,
            self.failures.len()
        );
        for failure in &self.failures {
            for error in &failure.errors {
                println!("  [FAIL] {}: {error}", failure.skill);
            }
        }
    }
}

fn print_warnings(warnings: &[String]) {
    for warning in warnings {
        println!("  warning: {warning}");
    }
}
