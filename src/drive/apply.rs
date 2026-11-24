//! Executes `plan::PlanItem`s. Integrator. Spec §6.3–§6.6, invariants
//! I1 (canonical untouched), I2 (links removed as links), I4 (no silent
//! fallback), I6 (atomic or reversible writes).

use std::collections::{BTreeMap, btree_map::Entry};
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use tempfile::TempDir;
use walkdir::WalkDir;

use crate::drive::hash::hash_directory;
use crate::drive::ignore::is_ignored;
use crate::drive::link::{self, Created, EntryKind, LinkKind};
use crate::drive::plan::{Action, PlanItem};
use crate::drive::state::{InstallRecord, State};
use crate::platform::paths::display_path;
use crate::registry::manifest::InstallMode;
use crate::{Error, Result};

/// How a connection ended up on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Connection {
    Link(LinkKind),
    Copy,
    /// A link was requested but could not be made (I4).
    CopyFallback {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Created(Connection),
    Updated,
    Removed,
    Converted(Connection),
    Unchanged,
}

/// Audit metadata stored on the record (v0.4.1 `audit_metadata`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AuditMeta {
    pub last_audit_at: String,
    pub last_audit_result_hash: String,
    pub audit_profile: String,
    pub audit_threshold: String,
}

/// A cleanup failure after the on-disk connection and state were committed.
#[derive(Debug)]
pub(crate) struct CleanupWarning {
    pub(crate) path: PathBuf,
    pub(crate) error: Error,
}

impl std::fmt::Display for CleanupWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "cleanup could not remove {}: {}",
            display_path(&self.path),
            self.error
        )
    }
}

#[derive(Debug)]
pub(crate) struct Applied {
    pub(crate) outcome: Outcome,
    pub(crate) cleanup_warnings: Vec<CleanupWarning>,
}

impl Applied {
    fn new(outcome: Outcome, cleanup_warning: Option<CleanupWarning>) -> Self {
        Self {
            outcome,
            cleanup_warnings: cleanup_warning.into_iter().collect(),
        }
    }

    pub(crate) fn print_cleanup_warnings(&self) {
        for warning in &self.cleanup_warnings {
            eprintln!("warning: {warning}");
        }
    }
}

/// One check snapshot per skill. Each host gets a same-volume staging copy
/// made from these bytes, even if the live canonical skill changes again.
#[derive(Debug)]
pub(crate) struct CopySnapshot {
    _temp: TempDir,
    path: PathBuf,
    hash: String,
}

impl CopySnapshot {
    fn new(source: &Path) -> Result<Self> {
        let temp = tempfile::Builder::new()
            .prefix(".my-skills-check-")
            .tempdir()
            .map_err(|err| Error::io(std::env::temp_dir(), err))?;
        // Validation and audit identify the skill from this directory name.
        let path = temp.path().join(source.file_name().unwrap_or_default());
        copy_tree(source, &path)?;
        let hash = hash_directory(&path)?;
        Ok(Self {
            _temp: temp,
            path,
            hash,
        })
    }
}

#[derive(Debug)]
pub(crate) enum PreparedConnection {
    Link {
        _temp: TempDir,
        entry: PathBuf,
        kind: LinkKind,
    },
    Copy {
        snapshot: Rc<CopySnapshot>,
        connection: Connection,
    },
}

impl PreparedConnection {
    pub(crate) fn check_snapshot(&self) -> Option<&Path> {
        match self {
            Self::Copy { snapshot, .. } => Some(&snapshot.path),
            Self::Link { .. } => None,
        }
    }
}

/// Prepare copies and discover any link fallbacks before the batch is applied.
/// Link checks still read canonical: later edits are visible live and caught
/// by the registry's pre-commit check.
pub(crate) fn prepare(
    item: &PlanItem,
    snapshots: &mut BTreeMap<String, Rc<CopySnapshot>>,
) -> Result<Option<PreparedConnection>> {
    prepare_with(item, snapshots, link::create_link)
}

fn prepare_with(
    item: &PlanItem,
    snapshots: &mut BTreeMap<String, Rc<CopySnapshot>>,
    create_link: impl FnOnce(&Path, &Path) -> Result<Created>,
) -> Result<Option<PreparedConnection>> {
    let connection = match (item.action, item.mode) {
        (Action::Create, InstallMode::Link) => {
            let temp = stage_near(&item.destination)?;
            let entry = temp.path().join(&item.skill);
            match create_link(&entry, &absolute(&item.source)?)? {
                Created::Link(kind) => {
                    return Ok(Some(PreparedConnection::Link {
                        _temp: temp,
                        entry,
                        kind,
                    }));
                }
                Created::NeedsCopy { reason } => Connection::CopyFallback { reason },
            }
        }
        (Action::Create | Action::Update | Action::Convert, InstallMode::Copy) => Connection::Copy,
        _ => return Ok(None),
    };
    let snapshot = match snapshots.entry(item.skill.clone()) {
        Entry::Occupied(existing) => existing.get().clone(),
        Entry::Vacant(new) => new
            .insert(Rc::new(CopySnapshot::new(&item.source)?))
            .clone(),
    };
    Ok(Some(PreparedConnection::Copy {
        snapshot,
        connection,
    }))
}

/// Apply one plan item and persist its state before reporting success.
/// CLI batches use prepare and apply_prepared to check the snapshot first.
pub fn apply(item: &PlanItem, state: &mut State, audit: Option<&AuditMeta>) -> Result<Outcome> {
    let prepared = prepare(item, &mut BTreeMap::new())?;
    let applied = apply_prepared(item, state, audit, prepared.as_ref())?;
    applied.print_cleanup_warnings();
    Ok(applied.outcome)
}

pub(crate) fn apply_prepared(
    item: &PlanItem,
    state: &mut State,
    audit: Option<&AuditMeta>,
    prepared: Option<&PreparedConnection>,
) -> Result<Applied> {
    let dest = &item.destination;
    match (item.action, prepared) {
        (Action::Create, Some(PreparedConnection::Link { entry, kind, .. })) => {
            let connection = Connection::Link(*kind);
            let record = record_for(item, &connection, hash_directory(&item.source)?, audit);
            replace_entry(entry, dest, EntryKind::Missing, || {
                commit_record(state, record).map_err(|err| Error::StateSave(Box::new(err)))
            })?;
            Ok(Applied::new(Outcome::Created(connection), None))
        }
        (
            Action::Create | Action::Update | Action::Convert,
            Some(PreparedConnection::Copy {
                snapshot,
                connection,
            }),
        ) => {
            let record = record_for(item, connection, snapshot.hash.clone(), audit);
            let stage = stage_near(dest)?;
            let entry = stage.path().join(&item.skill);
            copy_tree(&snapshot.path, &entry)?;
            let (expected, outcome) = match item.action {
                Action::Create => (EntryKind::Missing, Outcome::Created(connection.clone())),
                Action::Update => (EntryKind::Dir, Outcome::Updated),
                Action::Convert => {
                    let kind = link::entry_kind(dest)?;
                    if !matches!(kind, EntryKind::Link(_)) {
                        return Err(Error::Refused(format!(
                            "{} is no longer a linked directory; not converting",
                            dest.display()
                        )));
                    }
                    (kind, Outcome::Converted(connection.clone()))
                }
                _ => unreachable!("only copy writes have a snapshot"),
            };
            let warning = replace_entry(&entry, dest, expected, || {
                let result = commit_record(state, record);
                if item.action == Action::Convert {
                    result
                } else {
                    result.map_err(|err| Error::StateSave(Box::new(err)))
                }
            })?;
            Ok(Applied::new(outcome, warning))
        }
        (Action::Remove, _) => remove(item, state),
        (Action::Convert, None) if item.mode == InstallMode::Link => {
            convert_copy_to_link(item, state, audit)
        }
        (action, _) if action.writes() => Err(Error::Refused("connection was not prepared".into())),
        _ => Ok(Applied::new(Outcome::Unchanged, None)),
    }
}

fn remove(item: &PlanItem, state: &mut State) -> Result<Applied> {
    let dest = &item.destination;
    let backup = match link::entry_kind(dest)? {
        EntryKind::Missing => None,
        EntryKind::Link(_) | EntryKind::Dir => Some(move_to_backup(dest)?),
        EntryKind::File => {
            return Err(Error::Refused(format!(
                "{} is a file, not a managed connection",
                dest.display()
            )));
        }
    };
    let key = (item.skill.clone(), item.host.clone());
    if let Err(err) = commit_record_change(state, key, None) {
        restore_backup(backup.as_deref(), dest)?;
        return Err(Error::StateSave(Box::new(err)));
    }
    Ok(Applied::new(Outcome::Removed, cleanup_backup(backup)))
}

fn convert_copy_to_link(
    item: &PlanItem,
    state: &mut State,
    audit: Option<&AuditMeta>,
) -> Result<Applied> {
    let dest = &item.destination;
    let staged = stage_near(dest)?;
    let entry = staged.path().join(&item.skill);
    let canonical = absolute(&item.source)?;
    match link::create_link(&entry, &canonical)? {
        Created::Link(kind) => {
            let connection = Connection::Link(kind);
            let record = record_for(item, &connection, hash_directory(&item.source)?, audit);
            let warning = replace_entry(&entry, dest, EntryKind::Dir, || {
                commit_record(state, record)
            })?;
            Ok(Applied::new(Outcome::Converted(connection), warning))
        }
        Created::NeedsCopy { reason } => Err(Error::Link(format!(
            "cannot link {}: {reason}; the copy was kept",
            dest.display()
        ))),
    }
}

fn commit_record(state: &mut State, record: InstallRecord) -> Result<()> {
    commit_record_change(state, record.key(), Some(record))
}

/// Save a record change and restore the previous record in memory if it fails.
fn commit_record_change(
    state: &mut State,
    key: (String, String),
    record: Option<InstallRecord>,
) -> Result<()> {
    let previous = match record {
        Some(record) => state.installs.insert(key.clone(), record),
        None => state.installs.remove(&key),
    };
    if let Err(err) = state.save() {
        match previous {
            Some(previous) => state.put(previous),
            None => {
                state.installs.remove(&key);
            }
        }
        return Err(err);
    }
    Ok(())
}

/// Remove a managed connection: links with `remove_link`, copies only after
/// confirming they are real directories. The planner already checked the
/// copy's hash (I5).
fn remove_connection(dest: &Path) -> Result<()> {
    match link::entry_kind(dest)? {
        EntryKind::Missing => Ok(()),
        EntryKind::Link(_) => link::remove_link(dest),
        EntryKind::Dir => remove_real_dir(dest),
        EntryKind::File => Err(Error::Refused(format!(
            "{} is a file, not a managed connection",
            dest.display()
        ))),
    }
}

/// The only recursive delete in the crate. Refuses anything that is not a
/// real directory, so it can never reach through a link (I2).
pub(crate) fn remove_real_dir(path: &Path) -> Result<()> {
    match link::entry_kind(path)? {
        EntryKind::Missing => Ok(()),
        EntryKind::Dir => fs::remove_dir_all(path).map_err(|e| Error::io(path, e)),
        other => Err(Error::Refused(format!(
            "I2: {} is {other:?}, not a real directory; refusing recursive delete",
            path.display()
        ))),
    }
}

fn move_to_backup(dest: &Path) -> Result<PathBuf> {
    let backup = sibling(dest, "backup");
    fs::rename(dest, &backup).map_err(|err| Error::io(dest, err))?;
    Ok(backup)
}

fn restore_backup(backup: Option<&Path>, dest: &Path) -> Result<()> {
    if let Some(backup) = backup {
        fs::rename(backup, dest).map_err(|err| Error::io(backup, err))?;
    }
    Ok(())
}

fn cleanup_backup(backup: Option<PathBuf>) -> Option<CleanupWarning> {
    let path = backup?;
    remove_connection(&path)
        .err()
        .map(|error| CleanupWarning { path, error })
}

fn stage_near(dest: &Path) -> Result<TempDir> {
    let parent = dest
        .ancestors()
        .skip(1)
        .find(|p| p.is_dir())
        .unwrap_or(Path::new("."));
    tempfile::Builder::new()
        .prefix(".my-skills-staging-")
        .tempdir_in(parent)
        .map_err(|err| Error::io(parent, err))
}

/// Copy through a staging directory and retain the old copy until the swap.
pub fn copy_install(source: &Path, dest: &Path) -> Result<()> {
    let stage = stage_near(dest)?;
    let entry = stage.path().join("copy");
    copy_tree(source, &entry)?;
    let expected = if link::entry_kind(dest)? == EntryKind::Missing {
        EntryKind::Missing
    } else {
        EntryKind::Dir
    };
    if let Some(warning) = replace_entry(&entry, dest, expected, || Ok(()))? {
        eprintln!("warning: {warning}");
    }
    Ok(())
}

/// Swap entries using same-volume renames. Keep the previous connection
/// recoverable until the state save succeeds.
fn replace_entry(
    staged: &Path,
    dest: &Path,
    expected: EntryKind,
    commit: impl FnOnce() -> Result<()>,
) -> Result<Option<CleanupWarning>> {
    let kind = link::entry_kind(dest)?;
    if kind != expected {
        return Err(Error::Refused(format!(
            "{} is {kind:?}; expected {expected:?}",
            dest.display()
        )));
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|err| Error::io(parent, err))?;
    }
    let backup = (kind != EntryKind::Missing)
        .then(|| move_to_backup(dest))
        .transpose()?;
    if let Err(err) = fs::rename(staged, dest) {
        restore_backup(backup.as_deref(), dest)?;
        return Err(Error::io(dest, err));
    }
    if let Err(err) = commit() {
        fs::rename(dest, staged).map_err(|err| Error::io(dest, err))?;
        restore_backup(backup.as_deref(), dest)?;
        return Err(err);
    }
    Ok(cleanup_backup(backup))
}

/// Copy regular files and directories, skipping ignored entries. A link
/// inside the canonical skill is refused instead of followed; v0.4.1 import
/// already rejects such skills.
fn copy_tree(source: &Path, dest: &Path) -> Result<()> {
    if !source.is_dir() {
        return Err(Error::Link(format!(
            "source skill directory not found: {}",
            source.display()
        )));
    }
    fs::create_dir_all(dest).map_err(|e| Error::io(dest, e))?;
    for entry in WalkDir::new(source).follow_links(false).min_depth(1) {
        let entry = entry.map_err(|e| {
            let path = e.path().map(Path::to_path_buf).unwrap_or_default();
            Error::io(path, e.into())
        })?;
        let rel = entry
            .path()
            .strip_prefix(source)
            .expect("walkdir yields children of source");
        if is_ignored(rel) {
            continue;
        }
        let target = dest.join(rel);
        let file_type = entry.file_type();
        if file_type.is_symlink() {
            return Err(Error::Refused(format!(
                "{} is a link inside a canonical skill; not copying it",
                entry.path().display()
            )));
        } else if file_type.is_dir() {
            fs::create_dir_all(&target).map_err(|e| Error::io(&target, e))?;
        } else {
            fs::copy(entry.path(), &target).map_err(|e| Error::io(entry.path(), e))?;
        }
    }
    Ok(())
}

fn sibling(dest: &Path, purpose: &str) -> PathBuf {
    let name = dest
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    dest.with_file_name(format!(".{name}.{purpose}-{}-{nanos}", std::process::id()))
}

fn absolute(path: &Path) -> Result<PathBuf> {
    std::path::absolute(path).map_err(|e| Error::io(path, e))
}

fn record_for(
    item: &PlanItem,
    connection: &Connection,
    hash: String,
    audit: Option<&AuditMeta>,
) -> InstallRecord {
    let (mode, link_kind, fallback_reason) = match connection {
        Connection::Link(kind) => (InstallMode::Link, Some(*kind), None),
        Connection::Copy => (InstallMode::Copy, None, None),
        Connection::CopyFallback { reason } => (InstallMode::Copy, None, Some(reason.clone())),
    };
    let audit = audit.cloned().unwrap_or_default();
    InstallRecord {
        skill: item.skill.clone(),
        host: item.host.clone(),
        mode,
        source: item.source.to_string_lossy().into_owned(),
        destination: item.destination.to_string_lossy().into_owned(),
        source_hash: hash.clone(),
        installed_hash: hash,
        installed_at: utc_now(),
        source_type: "canonical".into(),
        source_url: String::new(),
        source_revision: String::new(),
        last_audit_at: audit.last_audit_at,
        last_audit_result_hash: audit.last_audit_result_hash,
        audit_profile: audit.audit_profile,
        audit_threshold: audit.audit_threshold,
        link_kind,
        fallback_reason,
    }
}

/// `YYYY-MM-DDTHH:MM:SSZ` in UTC, as v0.4.1 writes `installed_at`.
pub fn utc_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    format_utc(secs)
}

fn format_utc(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Civil-from-days (Howard Hinnant), valid for the proleptic Gregorian calendar.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3_600,
        rem % 3_600 / 60,
        rem % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_fallback_retries_after_state_save_failure_without_leaking_record() {
        let tmp = tempfile::tempdir().unwrap();
        let state_path = tmp.path().join("state.json");
        let blocker = state_path.with_file_name("state.json.tmp");
        fs::write(&blocker, "busy").unwrap();
        let mut state = State::load(Some(&state_path)).unwrap();
        let source = tmp.path().join("canonical");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("SKILL.md"), "canonical").unwrap();
        let item = PlanItem {
            skill: "alpha".into(),
            host: "claude".into(),
            source,
            destination: tmp.path().join("host").join("alpha"),
            action: Action::Create,
            reason: String::new(),
            mode: InstallMode::Link,
            source_hash: None,
        };
        let reason = "link unavailable";
        let force_fallback = |_: &Path, _: &Path| {
            Ok(Created::NeedsCopy {
                reason: reason.into(),
            })
        };
        let prepared = prepare_with(&item, &mut BTreeMap::new(), force_fallback).unwrap();
        let err = apply_prepared(&item, &mut state, None, prepared.as_ref()).unwrap_err();
        assert!(matches!(err, Error::StateSave(_)), "{err}");
        assert_eq!(
            link::entry_kind(&item.destination).unwrap(),
            EntryKind::Missing
        );
        assert!(state.get("alpha", "claude").is_none());
        assert!(!state_path.exists());

        fs::remove_file(blocker).unwrap();
        // A subsequent successful item using this state must not persist alpha.
        let other = PlanItem {
            host: "codex".into(),
            destination: tmp.path().join("other-host").join("alpha"),
            mode: InstallMode::Copy,
            ..item.clone()
        };
        assert_eq!(
            apply(&other, &mut state, None).unwrap(),
            Outcome::Created(Connection::Copy)
        );
        let persisted = State::load(Some(&state_path)).unwrap();
        assert_eq!(persisted, state);
        assert!(persisted.get("alpha", "claude").is_none());
        assert!(persisted.get("alpha", "codex").is_some());

        assert_eq!(
            apply_prepared(&item, &mut state, None, prepared.as_ref())
                .unwrap()
                .outcome,
            Outcome::Created(Connection::CopyFallback {
                reason: reason.into()
            }),
        );
        assert_eq!(link::entry_kind(&item.destination).unwrap(), EntryKind::Dir);
        let persisted = State::load(Some(&state_path)).unwrap();
        assert_eq!(persisted, state);
        let record = persisted.get("alpha", "claude").unwrap();
        assert_eq!(record.mode, InstallMode::Copy);
        assert_eq!(record.fallback_reason.as_deref(), Some(reason));
    }

    #[test]
    fn checked_snapshot_is_the_only_copy_source_across_hosts() {
        use crate::check::{audit, validate};
        use crate::drive::plan::{self, Status};
        use crate::registry::manifest::{CliOverrides, load_manifest};

        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let manifest = format!(
            "schema_version = 1\n[skills.alpha]\n\
             [targets.claude]\npath = {}\n[targets.codex]\npath = {}\n",
            serde_json::to_string(&root.join("claude")).unwrap(),
            serde_json::to_string(&root.join("codex")).unwrap(),
        );
        fs::write(root.join("my-skills.toml"), manifest).unwrap();
        let manifest = load_manifest(root, &CliOverrides::default()).unwrap();
        let skill = &manifest.skills["alpha"];
        let source = manifest.skill_dir("alpha");
        let state_path = root.join("state.json");
        let mut state = State::load(Some(&state_path)).unwrap();
        fs::create_dir_all(&source).unwrap();
        let original = "---\nname: alpha\ndescription: Test snapshot.\n---\n\nOriginal.";
        let skill_md = source.join("SKILL.md");
        fs::write(&skill_md, original).unwrap();

        let item =
            plan::plan_install_one(&manifest, skill, "claude", &state, InstallMode::Copy).unwrap();
        let mut snapshots = BTreeMap::new();
        let prepared = prepare(&item, &mut snapshots).unwrap().unwrap();
        let staged = prepared.check_snapshot().unwrap();
        assert!(validate::compose_validation(staged).ok());
        let policy = audit::policy_from_settings(&manifest.audit).unwrap();
        assert!(!audit::run_audit(staged, &policy).unwrap().blocked());
        let hash = hash_directory(staged).unwrap();

        fs::write(&skill_md, "Invalid edit after the check.").unwrap();
        let second =
            plan::plan_install_one(&manifest, skill, "codex", &state, InstallMode::Copy).unwrap();
        let second_prepared = prepare(&second, &mut snapshots).unwrap().unwrap();
        assert_eq!(second_prepared.check_snapshot(), Some(staged));
        for (item, prepared) in [(item, prepared), (second, second_prepared)] {
            let applied = apply_prepared(&item, &mut state, None, Some(&prepared)).unwrap();
            assert!(applied.cleanup_warnings.is_empty());
            assert_eq!(
                fs::read_to_string(item.destination.join("SKILL.md")).unwrap(),
                original
            );
            let record = state.get("alpha", &item.host).unwrap();
            assert_eq!(record.source_hash, hash);
            assert_eq!(record.installed_hash, hash);
            assert_eq!(
                plan::status_of(&manifest, skill, &item.host, &state).unwrap(),
                Status::Stale
            );
        }
        assert_eq!(State::load(Some(&state_path)).unwrap(), state);
    }

    #[test]
    fn utc_format_known_instants() {
        assert_eq!(format_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(format_utc(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(format_utc(1_795_398_144), "2026-11-23T01:42:24Z");
    }

    #[test]
    fn remove_real_dir_refuses_links() {
        let tmp = tempfile::tempdir().unwrap();
        let canonical = tmp.path().join("canonical");
        fs::create_dir_all(&canonical).unwrap();
        fs::write(canonical.join("SKILL.md"), "keep").unwrap();
        let dest = tmp.path().join("link");
        link::create_link(&dest, &canonical).unwrap();

        let err = remove_real_dir(&dest).unwrap_err();
        assert!(matches!(err, Error::Refused(_)), "{err}");
        assert!(canonical.join("SKILL.md").is_file());
    }

    #[test]
    fn copy_tree_skips_ignored_and_refuses_links() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        fs::create_dir_all(src.join("scripts")).unwrap();
        fs::create_dir_all(src.join("__pycache__")).unwrap();
        fs::write(src.join("SKILL.md"), "a").unwrap();
        fs::write(src.join(".DS_Store"), "x").unwrap();
        fs::write(src.join("__pycache__").join("m.pyc"), "x").unwrap();
        fs::write(src.join("scripts").join("run.sh"), "b").unwrap();

        let out = tmp.path().join("out");
        copy_tree(&src, &out).unwrap();
        assert!(out.join("SKILL.md").is_file());
        assert!(out.join("scripts").join("run.sh").is_file());
        assert!(!out.join(".DS_Store").exists());
        assert!(!out.join("__pycache__").exists());

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&out, src.join("loop")).unwrap();
            let err = copy_tree(&src, &tmp.path().join("out2")).unwrap_err();
            assert!(matches!(err, Error::Refused(_)), "{err}");
        }
    }
}
