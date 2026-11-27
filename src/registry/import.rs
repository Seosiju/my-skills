//! Skill import from external paths and host directories.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use cap_fs_ext::{DirExt, FollowSymlinks, OpenOptionsFollowExt, SystemTimeSpec};
use cap_std::ambient_authority;
use cap_std::fs::{Dir, OpenOptions};
use serde::Serialize;
use tempfile::{Builder, TempDir};

use crate::check::frontmatter::Node;
use crate::check::validate::{validate_name, validate_skill};
use crate::drive::apply::{CleanupWarning, remove_real_dir, utc_now};
use crate::drive::hash::hash_directory;
use crate::drive::link::{EntryKind, entry_kind, remove_link};
use crate::drive::state::{InstallRecord, State, StateLock, default_state_path};
use crate::registry::catalog::{display_path, field, metadata, table};
use crate::registry::edit::{self, ManifestEdit};
use crate::registry::manifest::{
    CliOverrides, InstallMode, MANIFEST_FILE, Manifest, load_manifest,
};
use crate::registry::root::RootResolution;
use crate::{Error, Result, platform::paths};

#[derive(Debug)]
pub(crate) enum Failure {
    Config(Error),
    Link { skill: String, path: PathBuf },
    Validation { skill: String, errors: Vec<String> },
    Blocked { skill: String, message: String },
}

impl From<Error> for Failure {
    fn from(err: Error) -> Self {
        Self::Config(err)
    }
}

type Operation<T> = std::result::Result<T, Failure>;

#[derive(Serialize)]
pub(crate) struct HostImportPlan {
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

pub(crate) fn plan_from_host(manifest: &Manifest, host: &str) -> Result<HostImportPlan> {
    let source_root = host_root(manifest, host)?;
    let children = match fs::read_dir(source_root) {
        Ok(children) => children,
        Err(err)
            if matches!(
                err.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
            ) =>
        {
            return Ok(HostImportPlan {
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
    let state = State::load(None)?;
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
        candidates.push(plan_candidate(manifest, host, &source, &state)?);
    }
    Ok(HostImportPlan {
        from_host: host.into(),
        source: display_path(source_root),
        candidates,
    })
}

fn plan_candidate(
    manifest: &Manifest,
    host: &str,
    source: &Path,
    state: &State,
) -> Result<Candidate> {
    let (name, description, hash, mut risks) = match open_source(source) {
        Ok(dir) => match visit(&dir, source, Path::new(""), None) {
            Ok(()) => {
                let (name, description) = candidate_metadata(source);
                (name, description, hash_directory(source)?, risks(source))
            }
            Err(failure) => unsafe_candidate(source, failure)?,
        },
        Err(failure) => unsafe_candidate(source, failure)?,
    };
    if let Some(reason) = foreign_record_reason(manifest, state, &name, host) {
        risks.push(Risk::error(reason));
    }
    if let Some(error) = invalid_registration_name(&name) {
        risks.push(Risk::error(error));
    }
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

fn foreign_record_reason(
    manifest: &Manifest,
    state: &State,
    name: &str,
    host: &str,
) -> Option<String> {
    state
        .get(name, host)
        .filter(|record| !record.belongs_to(&manifest.skills_dir()))
        .map(InstallRecord::foreign_reason)
}

fn unsafe_candidate(
    source: &Path,
    failure: Failure,
) -> Result<(String, String, String, Vec<Risk>)> {
    let message = match failure {
        Failure::Link { path, .. } => {
            format!("import source contains symlink: {}", display_path(&path))
        }
        Failure::Blocked { message, .. } => message,
        Failure::Config(err) => return Err(err),
        Failure::Validation { .. } => unreachable!("classifying source entries"),
    };
    Ok((
        basename(source).into(),
        String::new(),
        String::new(),
        vec![Risk::error(message)],
    ))
}

impl HostImportPlan {
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

pub(crate) struct ImportOutcome {
    pub name: String,
    pub canonical: PathBuf,
    pub enabled: bool,
    pub adopted_host: Option<String>,
    pub identical: bool,
    pub overwritten: bool,
    pub warnings: Vec<String>,
    pub cleanup_warning: Option<CleanupWarning>,
}

struct StagedImport {
    snapshot: Snapshot,
    name: String,
    canonical: PathBuf,
    old: Canonical,
    hash: String,
    status: &'static str,
    warnings: Vec<String>,
}

fn stage_import(
    manifest: &Manifest,
    source: &Path,
    force: bool,
    dry_run: bool,
    allow_non_dir_overwrite: bool,
    inline_conflict_hint: bool,
) -> Operation<StagedImport> {
    let snapshot = Snapshot::new(manifest, source, dry_run)?;
    let (name, _) = candidate_metadata(&snapshot.source);
    let validation = validate_skill(&snapshot.source);
    if !validation.ok() {
        return Err(Failure::Validation {
            skill: name,
            errors: validation.errors,
        });
    }
    if let Some(error) = invalid_registration_name(&name) {
        return Err(Failure::Blocked {
            skill: name,
            message: error,
        });
    }
    let canonical = manifest.skill_dir(&name);
    let old = Canonical::read(&canonical)?;
    let hash = hash_directory(&snapshot.source)?;
    let status = old.status(&hash);
    if status == "different" && !force {
        let message = if inline_conflict_hint {
            format!(
                "{name}: a different canonical skill already exists at {}; re-run with --force to overwrite it",
                display_path(&canonical)
            )
        } else {
            format!(
                "{name}: a different canonical skill already exists at\n  {}",
                display_path(&canonical)
            )
        };
        return Err(Failure::Blocked {
            skill: name,
            message,
        });
    }
    if !allow_non_dir_overwrite && matches!(old, Canonical::Other(_)) {
        return Err(Failure::Blocked {
            skill: name,
            message: format!(
                "canonical skill is not a real directory: {}",
                display_path(&canonical)
            ),
        });
    }
    Ok(StagedImport {
        snapshot,
        name,
        canonical,
        old,
        hash,
        status,
        warnings: validation.warnings,
    })
}

pub(crate) fn import_path(
    resolution: &RootResolution,
    source: &Path,
    enable: bool,
    force: bool,
    dry_run: bool,
) -> Operation<ImportOutcome> {
    let manifest = load_manifest(&resolution.root, &CliOverrides::default())?;
    let staged = stage_import(&manifest, source, force, dry_run, false, false)?;
    let current = manifest
        .skills
        .get(&staged.name)
        .is_some_and(|skill| skill.enabled);
    let (edit, enabled) = edit::prepare_import(
        &manifest.root.join(MANIFEST_FILE),
        &staged.name,
        enable,
        current,
        &enabled_hosts(&manifest),
        dry_run,
    )?;
    // A dry run uses the staged snapshot without acquiring the edit lock.
    let cleanup_warning = if dry_run {
        None
    } else {
        resolution.recheck_write()?;
        let warning = staged.snapshot.publish(
            &manifest,
            &staged.canonical,
            &staged.old,
            &staged.hash,
            &edit,
            || Ok(()),
        )?;
        resolution.remember();
        warning
    };
    Ok(ImportOutcome {
        name: staged.name,
        canonical: staged.canonical,
        enabled,
        adopted_host: None,
        identical: staged.status == "identical",
        overwritten: staged.status == "different",
        warnings: staged.warnings,
        cleanup_warning,
    })
}

pub(crate) fn import_from_host(
    resolution: &RootResolution,
    host: &str,
    skill: &str,
    enabled: bool,
    force: bool,
    dry_run: bool,
) -> Operation<ImportOutcome> {
    let registry_root = &resolution.root;
    let manifest = load_manifest(registry_root, &CliOverrides::default())?;
    let manifest_path = registry_root.join(MANIFEST_FILE);
    let state_path = default_state_path();
    ensure_outside_hosts(registry_root, target_paths(&manifest))?;
    ensure_outside_hosts(&state_path, target_paths(&manifest))?;
    // Adopting a host copy edits the manifest and state. Always take the edit
    // lock before the state lock; dry runs acquire neither lock.
    let edit_lock = (!dry_run)
        .then(|| edit::EditLock::acquire(&manifest_path))
        .transpose()?;
    let _state_lock = (!dry_run)
        .then(|| StateLock::acquire(&state_path))
        .transpose()?;
    if !dry_run {
        resolution.recheck_write()?;
    }
    let mut state = State::load(Some(&state_path))?;
    if let Some(reason) = foreign_record_reason(&manifest, &state, skill, host) {
        return Err(Failure::Blocked {
            skill: skill.into(),
            message: format!("{skill}: {reason}"),
        });
    }
    let root = host_root(&manifest, host)?;
    let path = Path::new(skill);
    if path.components().count() != 1 || path.file_name() != Some(path.as_os_str()) {
        return Err(Error::Manifest(format!(
            "invalid skill name '{skill}': expected one path component"
        ))
        .into());
    }
    let source = root.join(skill);
    let staged = stage_import(&manifest, &source, force, dry_run, true, true)?;
    if staged.status == "identical" {
        let validation = validate_skill(&staged.canonical);
        if !validation.ok() {
            return Err(Failure::Blocked {
                skill: staged.name.clone(),
                message: format!(
                    "{}: canonical validation failed after import: {}",
                    staged.name,
                    validation.errors.join("; ")
                ),
            });
        }
    }
    if staged.name != skill
        && let Some(reason) = foreign_record_reason(&manifest, &state, &staged.name, host)
    {
        return Err(Failure::Blocked {
            skill: staged.name.clone(),
            message: format!("{}: {reason}", staged.name),
        });
    }
    let cleanup_warning = if let Some(edit_lock) = edit_lock {
        let edit = edit::prepare_registration_locked(
            &manifest_path,
            &staged.name,
            enabled,
            &enabled_hosts(&manifest),
            edit_lock,
        )?;
        state.put(InstallRecord {
            skill: staged.name.clone(),
            host: host.into(),
            mode: InstallMode::Copy,
            source: display_path(&staged.canonical),
            destination: display_path(&source),
            source_hash: staged.hash.clone(),
            installed_hash: staged.hash.clone(),
            installed_at: utc_now(),
            source_type: "host".into(),
            source_url: display_path(&source),
            source_revision: staged.hash.clone(),
            link_kind: None,
            fallback_reason: None,
        });
        let warning = staged.snapshot.publish(
            &manifest,
            &staged.canonical,
            &staged.old,
            &staged.hash,
            &edit,
            || state.save(),
        )?;
        resolution.remember();
        warning
    } else {
        None
    };
    Ok(ImportOutcome {
        name: staged.name,
        canonical: staged.canonical,
        enabled,
        adopted_host: Some(host.into()),
        identical: staged.status == "identical",
        overwritten: staged.status == "different",
        warnings: staged.warnings,
        cleanup_warning,
    })
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
            .map(Node::to_text)
    };
    (
        if validate_name(name).is_empty() {
            name.into()
        } else {
            value("name")
                .filter(|name| validate_name(name).is_empty())
                .unwrap_or_else(|| name.into())
        },
        value("description").unwrap_or_default(),
    )
}

fn invalid_registration_name(name: &str) -> Option<String> {
    validate_name(name)
        .first()
        .map(|error| format!("invalid registry skill name {name:?}: {error}"))
}

fn risks(source: &Path) -> Vec<Risk> {
    let validation = validate_skill(source);
    let mut risks: Vec<_> = validation.errors.into_iter().map(Risk::error).collect();
    risks.extend(validation.warnings.into_iter().map(|message| Risk {
        severity: "warning",
        message,
        blocks: false,
    }));
    risks
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
    let skill = basename(path);
    match entry_kind(path)? {
        EntryKind::Link(_) => {
            return Err(Failure::Link {
                skill: skill.into(),
                path: PathBuf::from("."),
            });
        }
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
        Ok(meta) if meta.is_symlink() => Err(Failure::Link {
            skill: skill.into(),
            path: PathBuf::from("SKILL.md"),
        }),
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
    let skill = basename(root);
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
            return Err(Failure::Link {
                skill: skill.into(),
                path: rel,
            });
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
                        return Err(unsupported_entry(skill, &rel));
                    }
                    let mut file =
                        fs::File::create(&output).map_err(|err| Error::io(&output, err))?;
                    io::copy(&mut input, &mut file).map_err(|err| Error::io(&output, err))?;
                    apply_metadata(&file, &meta).map_err(|err| Error::io(&output, err))?;
                }
            }
            _ => return Err(unsupported_entry(skill, &rel)),
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

fn unsupported_entry(skill: &str, path: &Path) -> Failure {
    Failure::Blocked {
        skill: skill.into(),
        message: format!(
            "source contains an unsupported entry: {}",
            display_path(path)
        ),
    }
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
    fn new(manifest: &Manifest, source: &Path, dry_run: bool) -> Operation<Self> {
        let skill = basename(source);
        let dir = open_source(source)?;
        let resolved_source = physical_path(source)?.0;
        let data = physical_path(&paths::data_root())?.0;
        if same_or_below(&resolved_source, &data) || same_or_below(&data, &resolved_source) {
            return Err(Failure::Blocked {
                skill: skill.into(),
                message: "source overlaps the machine-local skill data directory".into(),
            });
        }
        // Check the complete tree before staging so a late link cannot leave
        // a partial copy in the registry, even in an ignored directory.
        visit(&dir, source, Path::new(""), None)?;
        let canonical_root = manifest.skills_dir();
        let (_, canonical_stage_parent) = physical_path(&canonical_root)?;
        ensure_outside_hosts(&canonical_root, target_paths(manifest))?;
        ensure_outside_hosts(&canonical_stage_parent, target_paths(manifest))?;
        ensure_outside_hosts(&manifest.root, target_paths(manifest))?;
        if same_or_below(&physical_path(&canonical_stage_parent)?.0, &resolved_source) {
            return Err(Failure::Blocked {
                skill: skill.into(),
                message: "source contains the registry staging directory".into(),
            });
        }
        // Dry runs never publish via rename, so stage under the OS temp dir
        // instead of touching the registry's skills/ directory.
        let stage_parent = if dry_run {
            std::env::temp_dir()
        } else {
            canonical_stage_parent
        };
        let temp = Builder::new()
            .prefix(".my-skills-")
            .tempdir_in(&stage_parent)
            .map_err(|err| Error::io(&stage_parent, err))?;
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
    ) -> Operation<Option<CleanupWarning>> {
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
            return Err(Failure::Blocked {
                skill: basename(dest).into(),
                message: format!(
                    "{} changed during the operation; re-run the command",
                    display_path(dest)
                ),
            });
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
        if !backed_up {
            return Ok(None);
        }
        let cleanup = match previous {
            Canonical::Other(EntryKind::Link(_)) => remove_link(&backup),
            Canonical::Other(EntryKind::File) => {
                fs::remove_file(&backup).map_err(|err| Error::io(&backup, err))
            }
            _ => remove_real_dir(&backup),
        };
        if let Err(error) = cleanup {
            // Keep any recovery files at the reported path.
            let _ = self.temp.keep();
            return Ok(Some(CleanupWarning {
                path: backup,
                error,
            }));
        }
        Ok(None)
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
