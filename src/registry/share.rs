//! Host-local skill share and import (v0.4.1 sharing.py, registry_commands.py). Stream H.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use cap_fs_ext::{DirExt, FollowSymlinks, OpenOptionsFollowExt, SystemTimeSpec};
use cap_std::ambient_authority;
use cap_std::fs::{Dir, OpenOptions};
use serde::Serialize;
use tempfile::{Builder, TempDir};

use crate::check::audit::{self, AuditGateResult, AuditPolicy, Severity};
use crate::check::frontmatter::Node;
use crate::check::validate::{compose_validation, validate_name};
use crate::drive::apply::utc_now;
use crate::drive::hash::hash_directory;
use crate::drive::link::{EntryKind, entry_kind, remove_link};
use crate::drive::state::{InstallRecord, State};
use crate::registry::catalog::{display_path, field, metadata, table};
use crate::registry::edit::{self, ManifestEdit};
use crate::registry::manifest::{InstallMode, MANIFEST_FILE, Manifest};
use crate::{Error, Result, platform::paths};

#[derive(Debug)]
pub(crate) enum Failure {
    Config(Error),
    Link(PathBuf),
    Validation(Vec<String>),
    Audit(AuditGateResult),
    Blocked(String),
}

impl From<Error> for Failure {
    fn from(err: Error) -> Self {
        Self::Config(err)
    }
}

type Operation<T> = std::result::Result<T, Failure>;

#[derive(Serialize)]
pub(crate) struct SharePlan {
    #[serde(rename = "from")]
    from_host: String,
    source: String,
    candidates: Vec<Candidate>,
}

#[derive(Serialize)]
struct Candidate {
    name: String,
    description: String,
    source: String,
    canonical: String,
    canonical_status: &'static str,
    content_hash: String,
    risks: Vec<Risk>,
    choices: &'static [&'static str],
    recommended: &'static str,
}

#[derive(Serialize)]
struct Risk {
    severity: &'static str,
    message: String,
    #[serde(skip)]
    blocks: bool,
}

impl Risk {
    fn error(message: String) -> Self {
        Self {
            severity: "error",
            message,
            blocks: true,
        }
    }
}

pub(crate) fn plan_share(manifest: &Manifest, host: &str) -> Result<SharePlan> {
    let source_root = host_root(manifest, host)?;
    let policy = audit::policy_from_settings(&manifest.audit)?;
    let children = match fs::read_dir(source_root) {
        Ok(children) => children,
        Err(err)
            if matches!(
                err.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
            ) =>
        {
            return Ok(SharePlan {
                from_host: host.into(),
                source: display_path(source_root),
                candidates: Vec::new(),
            });
        }
        Err(err) => return Err(Error::io(source_root, err)),
    };
    let mut children = children
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|err| Error::io(source_root, err))
        })
        .collect::<Result<Vec<_>>>()?;
    children.sort();
    let mut candidates = Vec::new();
    for source in children {
        match entry_kind(&source)? {
            EntryKind::Link(_) => {}
            EntryKind::Dir
                if matches!(
                    entry_kind(&source.join("SKILL.md"))?,
                    EntryKind::File | EntryKind::Link(_)
                ) => {}
            _ => continue,
        }
        candidates.push(plan_candidate(manifest, &source, &policy)?);
    }
    Ok(SharePlan {
        from_host: host.into(),
        source: display_path(source_root),
        candidates,
    })
}

fn plan_candidate(manifest: &Manifest, source: &Path, policy: &AuditPolicy) -> Result<Candidate> {
    let (name, description, hash, risks) = match open_source(source) {
        Ok(dir) => match visit(&dir, source, Path::new(""), None) {
            Ok(()) => {
                let (name, description) = candidate_metadata(source);
                (
                    name,
                    description,
                    hash_directory(source)?,
                    risks(source, policy, false)?,
                )
            }
            Err(failure) => unsafe_candidate(source, failure)?,
        },
        Err(failure) => unsafe_candidate(source, failure)?,
    };
    // Invalid frontmatter may propose a path outside the registry.
    let dest_name = if validate_name(&name).is_empty() {
        &name
    } else {
        basename(source)
    };
    let canonical = manifest.skill_dir(dest_name);
    let status = Canonical::read(&canonical)?.status(&hash);
    let (recommended, choices): (_, &[_]) =
        if risks.iter().any(|risk| risk.blocks) || status == "different" {
            ("skip", &["skip"])
        } else {
            ("share-enable", &["share-enable", "share-disable", "skip"])
        };
    Ok(Candidate {
        name,
        description,
        source: display_path(source),
        canonical: display_path(&canonical),
        canonical_status: status,
        content_hash: hash,
        risks,
        choices,
        recommended,
    })
}

fn unsafe_candidate(
    source: &Path,
    failure: Failure,
) -> Result<(String, String, String, Vec<Risk>)> {
    let message = match failure {
        Failure::Link(path) => format!("share source contains symlink: {}", display_path(&path)),
        Failure::Blocked(message) => message,
        Failure::Config(err) => return Err(err),
        Failure::Validation(_) | Failure::Audit(_) => unreachable!("classifying source entries"),
    };
    Ok((
        basename(source).into(),
        String::new(),
        String::new(),
        vec![Risk::error(message)],
    ))
}

impl SharePlan {
    pub(crate) fn table(&self) -> String {
        if self.candidates.is_empty() {
            return format!("no skills found in {}", self.source);
        }
        let headers = ["Skill", "Canonical", "Recommended", "Risks"].map(str::to_owned);
        let rows = self.candidates.iter().map(|item| {
            let risks = if item.risks.is_empty() {
                "none".into()
            } else {
                item.risks
                    .iter()
                    .map(|risk| format!("{}:{}", risk.severity, risk.message))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            vec![
                item.name.clone(),
                item.canonical_status.into(),
                item.recommended.into(),
                risks,
            ]
        });
        table(headers.into(), rows, false)
    }
}

fn host_root<'a>(manifest: &'a Manifest, host: &str) -> Result<&'a Path> {
    manifest
        .targets
        .get(host)
        .map(|target| target.path.as_path())
        .ok_or_else(|| Error::Manifest(format!("unknown host: {host}")))
}

fn candidate_metadata(source: &Path) -> (String, String) {
    let name = basename(source);
    let Ok(meta) = metadata(source) else {
        return (name.into(), String::new());
    };
    let value = |key| {
        field(&meta, key)
            .filter(|node| node.truthy())
            .map(Node::python_string)
    };
    (
        value("name").unwrap_or_else(|| name.into()),
        value("description").unwrap_or_default(),
    )
}

fn risks(source: &Path, policy: &AuditPolicy, skip_audit: bool) -> Result<Vec<Risk>> {
    let validation = compose_validation(source);
    let mut risks: Vec<_> = validation.errors.into_iter().map(Risk::error).collect();
    risks.extend(validation.warnings.into_iter().map(|message| Risk {
        severity: "warning",
        message,
        blocks: false,
    }));
    if !skip_audit {
        let result = audit::run_audit(source, policy)?;
        for finding in result.findings {
            let blocks = finding.severity == Severity::Critical
                || result
                    .threshold
                    .is_some_and(|threshold| finding.severity >= threshold);
            let identity = format!("{}: {}", finding.file, finding.message);
            if let Some(existing) = risks
                .iter_mut()
                .find(|risk| risk_identity(&risk.message) == identity)
            {
                existing.blocks |= blocks;
            } else {
                risks.push(Risk {
                    severity: finding.severity.label(),
                    message: format!("{}: {identity}", finding.rule_id),
                    blocks,
                });
            }
        }
    }
    Ok(risks)
}

fn risk_identity(message: &str) -> &str {
    message
        .split_once(": ")
        .filter(|(_, rest)| rest.contains(": "))
        .map_or(message, |(_, rest)| rest)
}

pub(crate) struct ImportOutcome {
    pub name: String,
    pub canonical: PathBuf,
    pub enabled: bool,
    pub identical: bool,
    pub overwritten: bool,
}

pub(crate) fn import(
    manifest: &Manifest,
    source: &Path,
    enable: bool,
    force: bool,
    skip_audit: bool,
    dry_run: bool,
    on_audit_skip: impl FnOnce(),
) -> Operation<ImportOutcome> {
    let snapshot = if dry_run {
        Snapshot::new_dry_run(manifest, source)?
    } else {
        Snapshot::new(manifest, source)?
    };
    let validation = compose_validation(&snapshot.source);
    if !validation.ok() {
        return Err(Failure::Validation(validation.errors));
    }
    let policy = audit::policy_from_settings(&manifest.audit)?;
    let gate = audit::audit_skills(std::slice::from_ref(&snapshot.source), &policy, skip_audit)?;
    if gate.blocked() {
        return Err(Failure::Audit(gate));
    }
    if gate.skipped {
        on_audit_skip();
    }
    let meta = metadata(&snapshot.source).map_err(Failure::Blocked)?;
    let name = field(&meta, "name")
        .expect("name checked by validation")
        .python_string();
    let canonical = manifest.skill_dir(&name);
    let old = Canonical::read(&canonical)?;
    let hash = hash_directory(&snapshot.source)?;
    let status = old.status(&hash);
    if status == "different" && !force {
        return Err(Failure::Blocked(format!(
            "{name}: a different canonical skill already exists at\n  {}",
            display_path(&canonical)
        )));
    }
    if matches!(old, Canonical::Other(_)) {
        return Err(Failure::Blocked(format!(
            "canonical skill is not a real directory: {}",
            display_path(&canonical)
        )));
    }
    let current = manifest
        .skills
        .get(&name)
        .is_some_and(|skill| skill.enabled);
    let enabled = if dry_run {
        edit::preview_import(
            &manifest.root.join(MANIFEST_FILE),
            &name,
            enable,
            current,
            &enabled_hosts(manifest),
        )?
    } else {
        let (edit, enabled) = edit::prepare_import(
            &manifest.root.join(MANIFEST_FILE),
            &name,
            enable,
            current,
            &enabled_hosts(manifest),
        )?;
        snapshot.publish(manifest, &canonical, &old, &hash, &edit, || Ok(()))?;
        enabled
    };
    Ok(ImportOutcome {
        name,
        canonical,
        enabled,
        identical: status == "identical",
        overwritten: status == "different",
    })
}

pub(crate) struct ShareOutcome {
    pub name: String,
    pub canonical: PathBuf,
}

pub(crate) fn share(
    manifest: &Manifest,
    state: &mut State,
    host: &str,
    skill: &str,
    enabled: bool,
    force: bool,
    skip_audit: bool,
) -> Operation<ShareOutcome> {
    let root = host_root(manifest, host)?;
    let path = Path::new(skill);
    if path.components().count() != 1 || path.file_name() != Some(path.as_os_str()) {
        return Err(Error::Manifest(format!(
            "invalid skill name '{skill}': expected one path component"
        ))
        .into());
    }
    let source = root.join(skill);
    let snapshot = Snapshot::new(manifest, &source)?;
    let policy = audit::policy_from_settings(&manifest.audit)?;
    let (name, _) = candidate_metadata(&snapshot.source);
    let errors = risks(&snapshot.source, &policy, skip_audit)?
        .into_iter()
        .filter(|risk| risk.blocks)
        .map(|risk| risk.message)
        .collect::<Vec<_>>();
    if !errors.is_empty() {
        return Err(Failure::Validation(vec![format!(
            "{name}: validation failed: {}",
            errors.join("; ")
        )]));
    }
    let canonical = manifest.skill_dir(&name);
    let old = Canonical::read(&canonical)?;
    let hash = hash_directory(&snapshot.source)?;
    if old.status(&hash) == "different" && !force {
        return Err(Failure::Blocked(format!(
            "{name}: a different canonical skill already exists at {}; re-run with --force to overwrite it",
            display_path(&canonical)
        )));
    }
    if old.status(&hash) == "identical" {
        let validation = compose_validation(&canonical);
        if !validation.ok() {
            return Err(Failure::Blocked(format!(
                "{name}: canonical validation failed after import: {}",
                validation.errors.join("; ")
            )));
        }
    }
    let edit = edit::prepare_registration(
        &manifest.root.join(MANIFEST_FILE),
        &name,
        enabled,
        &enabled_hosts(manifest),
    )?;
    let audit = audit::run_audit(
        if old.status(&hash) == "identical" {
            &canonical
        } else {
            &snapshot.source
        },
        &policy,
    )?;
    ensure_outside_hosts(&state.path, target_paths(manifest))?;
    // State was initially loaded for the CLI's error ordering. Refresh it
    // with the manifest lock held so another share's adoption is retained.
    *state = State::load(Some(&state.path))?;
    let now = utc_now();
    state.put(InstallRecord {
        skill: name.clone(),
        host: host.into(),
        mode: InstallMode::Copy,
        source: display_path(&canonical),
        destination: display_path(&source),
        source_hash: hash.clone(),
        installed_hash: hash.clone(),
        installed_at: now.clone(),
        source_type: "host".into(),
        source_url: display_path(&source),
        source_revision: hash.clone(),
        last_audit_at: now,
        last_audit_result_hash: audit.result_hash(),
        audit_profile: audit.profile,
        audit_threshold: audit.threshold.map_or("none", Severity::label).into(),
        link_kind: None,
        fallback_reason: None,
    });
    snapshot.publish(manifest, &canonical, &old, &hash, &edit, || state.save())?;
    Ok(ShareOutcome { name, canonical })
}

fn enabled_hosts(manifest: &Manifest) -> Vec<String> {
    manifest
        .targets
        .values()
        .filter(|target| target.enabled)
        .map(|target| target.name.clone())
        .collect()
}

pub(crate) fn basename(path: &Path) -> &str {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
}

fn open_source(path: &Path) -> Operation<Dir> {
    match entry_kind(path)? {
        EntryKind::Link(_) => return Err(Failure::Link(PathBuf::from("."))),
        EntryKind::Dir => {}
        _ => return Err(not_a_skill(path)),
    }
    let (parent, name) = if let Some(name) = path.file_name() {
        (
            path.parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new(".")),
            name,
        )
    } else {
        (path, ".".as_ref())
    };
    let dir =
        Dir::open_ambient_dir(parent, ambient_authority()).map_err(|err| Error::io(parent, err))?;
    let dir = dir
        .open_dir_nofollow(name)
        .map_err(|err| Error::io(path, err))?;
    match dir.symlink_metadata("SKILL.md") {
        Ok(meta) if meta.is_symlink() => Err(Failure::Link(PathBuf::from("SKILL.md"))),
        Ok(meta) if meta.is_file() => Ok(dir),
        Ok(_) => Err(not_a_skill(path)),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Err(not_a_skill(path)),
        Err(err) => Err(Error::io(path.join("SKILL.md"), err).into()),
    }
}

fn not_a_skill(path: &Path) -> Failure {
    Error::Manifest(format!(
        "{} is not an Agent Skill (no SKILL.md)",
        display_path(path)
    ))
    .into()
}

// Use held directory handles for traversal, and no-follow opens for both kinds
// of entries. A link substituted after entry_kind cannot redirect the copy.
fn visit(dir: &Dir, root: &Path, relative: &Path, dest: Option<&Path>) -> Operation<()> {
    let source = root.join(relative);
    let mut entries = dir
        .entries()
        .map_err(|err| Error::io(&source, err))?
        .map(|entry| {
            entry
                .map(|entry| entry.file_name())
                .map_err(|err| Error::io(&source, err))
        })
        .collect::<Result<Vec<_>>>()?;
    entries.sort();
    for name in entries {
        let path = source.join(&name);
        let rel = relative.join(&name);
        let kind = entry_kind(&path)?;
        let meta = dir
            .symlink_metadata(&name)
            .map_err(|err| Error::io(&path, err))?;
        if matches!(kind, EntryKind::Link(_)) || meta.is_symlink() {
            return Err(Failure::Link(rel));
        }
        let output = dest.map(|dest| dest.join(&name));
        match kind {
            EntryKind::Dir if meta.is_dir() => {
                let child = dir
                    .open_dir_nofollow(&name)
                    .map_err(|err| Error::io(&path, err))?;
                if let Some(output) = &output {
                    fs::create_dir(output).map_err(|err| Error::io(output, err))?;
                }
                visit(&child, root, &rel, output.as_deref())?;
                if let Some(output) = &output {
                    let parent = output.parent().expect("copy has a destination parent");
                    let copied = Dir::open_ambient_dir(parent, ambient_authority())
                        .map_err(|err| Error::io(parent, err))?;
                    copied
                        .set_times(
                            &name,
                            meta.accessed().ok().map(SystemTimeSpec::Absolute),
                            meta.modified().ok().map(SystemTimeSpec::Absolute),
                        )
                        .and_then(|()| copied.set_permissions(&name, meta.permissions()))
                        .map_err(|err| Error::io(output, err))?;
                }
            }
            EntryKind::File if meta.is_file() => {
                if let Some(output) = output {
                    let mut options = OpenOptions::new();
                    options.read(true).follow(FollowSymlinks::No);
                    let mut input = dir
                        .open_with(&name, &options)
                        .map_err(|err| Error::io(&path, err))?;
                    if !input
                        .metadata()
                        .map_err(|err| Error::io(&path, err))?
                        .is_file()
                    {
                        return Err(unsupported_entry(&rel));
                    }
                    let mut file =
                        fs::File::create(&output).map_err(|err| Error::io(&output, err))?;
                    io::copy(&mut input, &mut file).map_err(|err| Error::io(&output, err))?;
                    apply_metadata(&file, &meta).map_err(|err| Error::io(&output, err))?;
                }
            }
            _ => return Err(unsupported_entry(&rel)),
        }
    }
    Ok(())
}

fn apply_metadata(file: &fs::File, meta: &cap_std::fs::Metadata) -> io::Result<()> {
    let mut times = fs::FileTimes::new();
    if let Ok(modified) = meta.modified() {
        times = times.set_modified(modified.into_std());
    }
    if let Ok(accessed) = meta.accessed() {
        times = times.set_accessed(accessed.into_std());
    }
    file.set_times(times)?;
    file.set_permissions(meta.permissions().into_std(file)?)
}

fn unsupported_entry(path: &Path) -> Failure {
    Failure::Blocked(format!(
        "source contains an unsupported entry: {}",
        display_path(path)
    ))
}

#[derive(PartialEq, Eq)]
enum Canonical {
    Missing,
    Directory(String),
    Other(EntryKind),
}

impl Canonical {
    fn read(path: &Path) -> Result<Self> {
        Ok(match entry_kind(path)? {
            EntryKind::Dir => Self::Directory(hash_directory(path)?),
            EntryKind::Missing => Self::Missing,
            kind => Self::Other(kind),
        })
    }

    fn status(&self, hash: &str) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Directory(digest) if digest == hash => "identical",
            _ => "different",
        }
    }
}

struct Snapshot {
    temp: TempDir,
    source: PathBuf,
}

impl Snapshot {
    fn check_only(manifest: &Manifest, source: &Path) -> Operation<(Dir, PathBuf)> {
        let dir = open_source(source)?;
        let resolved_source = physical_path(source)?.0;
        let data = physical_path(&paths::data_root())?.0;
        if same_or_below(&resolved_source, &data) || same_or_below(&data, &resolved_source) {
            return Err(Failure::Blocked(
                "source overlaps the machine-local skill data directory".into(),
            ));
        }
        // Check the complete tree before staging so a late link cannot leave
        // a partial copy in the registry, even in an ignored directory.
        visit(&dir, source, Path::new(""), None)?;
        let canonical_root = manifest.skills_dir();
        let (_, stage_parent) = physical_path(&canonical_root)?;
        ensure_outside_hosts(&canonical_root, target_paths(manifest))?;
        ensure_outside_hosts(&stage_parent, target_paths(manifest))?;
        ensure_outside_hosts(&manifest.root, target_paths(manifest))?;
        if same_or_below(&physical_path(&stage_parent)?.0, &resolved_source) {
            return Err(Failure::Blocked(
                "source contains the registry staging directory".into(),
            ));
        }
        Ok((dir, stage_parent))
    }

    fn new(manifest: &Manifest, source: &Path) -> Operation<Self> {
        Self::stage(manifest, source, false)
    }

    fn new_dry_run(manifest: &Manifest, source: &Path) -> Operation<Self> {
        Self::stage(manifest, source, true)
    }

    fn stage(manifest: &Manifest, source: &Path, outside_registry: bool) -> Operation<Self> {
        let (dir, stage_parent) = Self::check_only(manifest, source)?;
        let temp = if outside_registry {
            Builder::new()
                .prefix(".my-skills-")
                .tempdir()
                .map_err(|err| Error::io(std::env::temp_dir(), err))?
        } else {
            Builder::new()
                .prefix(".my-skills-")
                .tempdir_in(&stage_parent)
                .map_err(|err| Error::io(&stage_parent, err))?
        };
        let copied = temp
            .path()
            .join("snapshot")
            .join(source.file_name().unwrap_or(".".as_ref()));
        fs::create_dir_all(&copied).map_err(|err| Error::io(&copied, err))?;
        visit(&dir, source, Path::new(""), Some(&copied))?;
        Ok(Self {
            temp,
            source: copied,
        })
    }

    fn publish(
        self,
        manifest: &Manifest,
        dest: &Path,
        previous: &Canonical,
        source_hash: &str,
        edit: &ManifestEdit,
        finish: impl FnOnce() -> Result<()>,
    ) -> Operation<()> {
        let parent = dest.parent().expect("canonical skill has a parent");
        let path = physical_path(parent)?
            .0
            .join(dest.file_name().expect("canonical skill has a name"));
        for host in target_paths(manifest) {
            let host = physical_path(&host)?.0;
            if same_or_below(&host, &path) || same_or_below(&path, &host) {
                return Err(host_write(dest).into());
            }
        }
        if Canonical::read(dest)? != *previous {
            return Err(Failure::Blocked(format!(
                "{} changed during the operation; re-run the command",
                display_path(dest)
            )));
        }
        let status = previous.status(source_hash);
        let backup = self.temp.path().join("previous");
        let swapped = status != "identical";
        let backed_up = status == "different";
        if swapped {
            fs::create_dir_all(parent).map_err(|err| Error::io(parent, err))?;
            if backed_up {
                fs::rename(dest, &backup).map_err(|err| Error::io(dest, err))?;
            }
            if let Err(err) = fs::rename(&self.source, dest) {
                if let Err(rollback) = restore_backup(backed_up, &backup, dest) {
                    return Err(self.retain_backup(dest, rollback));
                }
                return Err(Error::io(dest, err).into());
            }
        }
        let mut manifest_written = false;
        let result = edit.commit().and_then(|()| {
            manifest_written = true;
            finish()
        });
        if let Err(err) = result {
            let manifest_rollback = if manifest_written {
                edit.rollback()
            } else {
                Ok(())
            };
            if swapped {
                if let Err(rollback) = fs::rename(dest, &self.source) {
                    return Err(self.retain_backup(dest, rollback));
                }
                if let Err(rollback) = restore_backup(backed_up, &backup, dest) {
                    return Err(self.retain_backup(dest, rollback));
                }
            }
            manifest_rollback?;
            return Err(err.into());
        }
        if matches!(previous, Canonical::Other(EntryKind::Link(_))) {
            remove_link(&backup)?;
        }
        Ok(())
    }

    fn retain_backup(self, dest: &Path, err: io::Error) -> Failure {
        let recovery = self.temp.keep();
        Error::Manifest(format!(
            "could not restore {}: {err}; recovery files kept at {}",
            display_path(dest),
            display_path(&recovery)
        ))
        .into()
    }
}

fn restore_backup(backed_up: bool, backup: &Path, dest: &Path) -> io::Result<()> {
    if backed_up {
        fs::rename(backup, dest)?;
    }
    Ok(())
}

pub(crate) fn target_paths(manifest: &Manifest) -> impl Iterator<Item = PathBuf> + '_ {
    manifest.targets.values().map(|target| target.path.clone())
}

pub(crate) fn ensure_outside_hosts(
    path: &Path,
    hosts: impl IntoIterator<Item = PathBuf>,
) -> Result<()> {
    let physical = physical_path(path)?.0;
    for host in hosts {
        if same_or_below(&physical, &physical_path(&host)?.0) {
            return Err(host_write(path));
        }
    }
    Ok(())
}

fn same_or_below(path: &Path, parent: &Path) -> bool {
    if cfg!(any(windows, target_os = "macos")) {
        let path = display_path(path).to_lowercase();
        let parent = display_path(parent).to_lowercase();
        Path::new(&path).starts_with(parent)
    } else {
        path.starts_with(parent)
    }
}

fn host_write(path: &Path) -> Error {
    Error::Refused(format!(
        "registry write overlaps a host directory: {}",
        display_path(path)
    ))
}

/// Resolve links in an existing prefix, retaining any missing suffix. Return
/// the path for overlap checks and the closest existing ancestor for staging.
fn physical_path(path: &Path) -> Result<(PathBuf, PathBuf)> {
    let absolute = std::path::absolute(path).map_err(|err| Error::io(path, err))?;
    let mut missing = Vec::new();
    for ancestor in absolute.ancestors().filter(|p| !p.as_os_str().is_empty()) {
        match fs::canonicalize(ancestor) {
            Ok(existing) => {
                let full = missing
                    .iter()
                    .rev()
                    .fold(existing.clone(), |resolved, name| resolved.join(name));
                return Ok((full, existing));
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                if let Some(name) = ancestor.file_name() {
                    missing.push(name);
                }
            }
            Err(err) => return Err(Error::io(ancestor, err)),
        }
    }
    Err(Error::Manifest(format!(
        "no existing ancestor for {}",
        display_path(path)
    )))
}
