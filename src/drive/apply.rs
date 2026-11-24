//! Executes `plan::PlanItem`s. Integrator. Spec §6.3–§6.6, invariants
//! I1 (canonical untouched), I2 (links removed as links), I4 (no silent
//! fallback), I6 (atomic or reversible writes).

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use walkdir::WalkDir;

use crate::drive::hash::hash_directory;
use crate::drive::ignore::is_ignored;
use crate::drive::link::{self, Created, EntryKind, LinkKind};
use crate::drive::plan::{Action, PlanItem};
use crate::drive::state::{InstallRecord, State};
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

/// Apply one plan item and persist its state before reporting success.
/// Blocked and informational items are not applied.
pub fn apply(item: &PlanItem, state: &mut State, audit: Option<&AuditMeta>) -> Result<Outcome> {
    match item.action {
        Action::Create => create(item, state, audit, link::create_link),
        Action::Update => {
            copy_install_with(&item.source, &item.destination, || {
                commit_install(item, state, &Connection::Copy, audit)
            })?;
            Ok(Outcome::Updated)
        }
        Action::Remove => remove(item, state),
        Action::Convert => convert(item, state, audit),
        _ => Ok(Outcome::Unchanged),
    }
}

fn create(
    item: &PlanItem,
    state: &mut State,
    audit: Option<&AuditMeta>,
    create_link: impl FnOnce(&Path, &Path) -> Result<Created>,
) -> Result<Outcome> {
    let dest = &item.destination;
    let connection = match item.mode {
        InstallMode::Copy => Connection::Copy,
        InstallMode::Link => match create_link(dest, &absolute(&item.source)?)? {
            Created::Link(kind) => {
                let connection = Connection::Link(kind);
                if let Err(err) = commit_install(item, state, &connection, audit) {
                    link::remove_link(dest)?;
                    return Err(err);
                }
                return Ok(Outcome::Created(connection));
            }
            Created::NeedsCopy { reason } => Connection::CopyFallback { reason },
        },
    };
    copy_install_with(&item.source, dest, || {
        commit_install(item, state, &connection, audit)
    })?;
    Ok(Outcome::Created(connection))
}

fn remove(item: &PlanItem, state: &mut State) -> Result<Outcome> {
    let dest = &item.destination;
    let backup = match link::entry_kind(dest)? {
        EntryKind::Missing => None,
        EntryKind::Link(_) | EntryKind::Dir => {
            // Rename the entry itself so even a broken link can be restored.
            let backup = sibling(dest, "backup");
            fs::rename(dest, &backup).map_err(|e| Error::io(dest, e))?;
            Some(backup)
        }
        EntryKind::File => {
            return Err(Error::Refused(format!(
                "{} is a file, not a managed connection",
                dest.display()
            )));
        }
    };
    let key = (item.skill.clone(), item.host.clone());
    if let Err(err) = commit_record_change(state, key, None) {
        if let Some(backup) = backup {
            fs::rename(&backup, dest).map_err(|e| Error::io(&backup, e))?;
        }
        return Err(Error::StateSave(Box::new(err)));
    }
    if let Some(backup) = backup {
        remove_connection(&backup)?;
    }
    Ok(Outcome::Removed)
}

fn commit_install(
    item: &PlanItem,
    state: &mut State,
    connection: &Connection,
    audit: Option<&AuditMeta>,
) -> Result<()> {
    let (mode, kind, reason) = match connection {
        Connection::Link(kind) => (InstallMode::Link, Some(*kind), None),
        Connection::Copy => (InstallMode::Copy, None, None),
        Connection::CopyFallback { reason } => (InstallMode::Copy, None, Some(reason.clone())),
    };
    let record = record_for(item, mode, kind, reason, audit)?;
    commit_record(state, record).map_err(|err| Error::StateSave(Box::new(err)))
}

fn convert(item: &PlanItem, state: &mut State, audit: Option<&AuditMeta>) -> Result<Outcome> {
    match item.mode {
        InstallMode::Link => convert_copy_to_link(item, state, audit),
        InstallMode::Copy => convert_link_to_copy(item, state, audit),
    }
}

fn convert_copy_to_link(
    item: &PlanItem,
    state: &mut State,
    audit: Option<&AuditMeta>,
) -> Result<Outcome> {
    let dest = &item.destination;
    if link::entry_kind(dest)? != EntryKind::Dir {
        return Err(Error::Refused(format!(
            "{} is not a copied directory; not converting",
            dest.display()
        )));
    }
    let canonical = absolute(&item.source)?;
    let backup = sibling(dest, "backup");
    fs::rename(dest, &backup).map_err(|e| Error::io(dest, e))?;
    let created = match link::create_link(dest, &canonical) {
        Ok(created) => created,
        Err(err) => {
            restore(&backup, dest)?;
            return Err(err);
        }
    };
    match created {
        Created::Link(kind) => {
            // Spec §6.6 order: link, then state saved, then backup removed.
            // If the record cannot be saved, undo the link and keep the copy
            // so disk and state.json stay consistent (I6).
            let committed = record_for(item, InstallMode::Link, Some(kind), None, audit)
                .and_then(|record| commit_record(state, record));
            if let Err(err) = committed {
                restore(&backup, dest)?;
                return Err(err);
            }
            remove_real_dir(&backup)?;
            Ok(Outcome::Converted(Connection::Link(kind)))
        }
        Created::NeedsCopy { reason } => {
            restore(&backup, dest)?;
            Err(Error::Link(format!(
                "cannot link {}: {reason}; the copy was kept",
                dest.display()
            )))
        }
    }
}

fn convert_link_to_copy(
    item: &PlanItem,
    state: &mut State,
    audit: Option<&AuditMeta>,
) -> Result<Outcome> {
    let dest = &item.destination;
    let staged = sibling(dest, "staging");
    copy_tree(&item.source, &staged)?;
    if let Err(err) = link::remove_link(dest) {
        remove_real_dir(&staged)?;
        return Err(err);
    }
    if let Err(err) = fs::rename(&staged, dest) {
        // Put the link back so the host keeps working.
        let _ = link::create_link(dest, &absolute(&item.source)?);
        let _ = remove_real_dir(&staged);
        return Err(Error::io(dest, err));
    }
    // Save before reporting success; on failure put the link back so disk
    // and state.json agree (I6). The staged copy was the canonical content,
    // so removing it loses nothing.
    let committed = record_for(item, InstallMode::Copy, None, None, audit)
        .and_then(|record| commit_record(state, record));
    if let Err(err) = committed {
        remove_real_dir(dest)?;
        link::create_link(dest, &absolute(&item.source)?)?;
        return Err(err);
    }
    Ok(Outcome::Converted(Connection::Copy))
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

fn restore(backup: &Path, dest: &Path) -> Result<()> {
    if link::entry_kind(dest)? != EntryKind::Missing {
        link::remove_link(dest)?;
    }
    fs::rename(backup, dest).map_err(|e| Error::io(dest, e))
}

/// Copy `source` to `dest` through a staging directory; an existing copy is
/// moved aside first and restored if the swap fails (I6).
pub fn copy_install(source: &Path, dest: &Path) -> Result<()> {
    copy_install_with(source, dest, || Ok(()))
}

/// Keep the previous copy until the replacement has been committed.
fn copy_install_with(
    source: &Path,
    dest: &Path,
    commit: impl FnOnce() -> Result<()>,
) -> Result<()> {
    if !source.is_dir() {
        return Err(Error::Link(format!(
            "source skill directory not found: {}",
            source.display()
        )));
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    let staged = sibling(dest, "staging");
    if let Err(err) = copy_tree(source, &staged) {
        let _ = remove_real_dir(&staged);
        return Err(err);
    }
    let backup = match link::entry_kind(dest)? {
        EntryKind::Missing => None,
        EntryKind::Dir => {
            let backup = sibling(dest, "backup");
            fs::rename(dest, &backup).map_err(|e| Error::io(dest, e))?;
            Some(backup)
        }
        other => {
            remove_real_dir(&staged)?;
            return Err(Error::Refused(format!(
                "{} is {other:?}; copy install replaces only directories",
                dest.display()
            )));
        }
    };
    if let Err(err) = fs::rename(&staged, dest) {
        if let Some(backup) = &backup {
            fs::rename(backup, dest).map_err(|e| Error::io(dest, e))?;
        }
        let _ = remove_real_dir(&staged);
        return Err(Error::io(dest, err));
    }
    if let Err(err) = commit() {
        fs::rename(dest, &staged).map_err(|e| Error::io(dest, e))?;
        if let Some(backup) = backup {
            fs::rename(&backup, dest).map_err(|e| Error::io(&backup, e))?;
        }
        remove_real_dir(&staged)?;
        return Err(err);
    }
    if let Some(backup) = backup {
        remove_real_dir(&backup)?;
    }
    Ok(())
}

/// Copy regular files and directories, skipping ignored entries. A link
/// inside the canonical skill is refused instead of followed; v0.4.1 import
/// already rejects such skills.
fn copy_tree(source: &Path, dest: &Path) -> Result<()> {
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
    mode: InstallMode,
    link_kind: Option<LinkKind>,
    fallback_reason: Option<String>,
    audit: Option<&AuditMeta>,
) -> Result<InstallRecord> {
    let source_hash = match &item.source_hash {
        Some(hash) => hash.clone(),
        None => hash_directory(&item.source)?,
    };
    let installed_hash = match mode {
        // A link's installed content IS the canonical content.
        InstallMode::Link => source_hash.clone(),
        InstallMode::Copy => hash_directory(&item.destination)?,
    };
    let audit = audit.cloned().unwrap_or_default();
    Ok(InstallRecord {
        skill: item.skill.clone(),
        host: item.host.clone(),
        mode,
        source: item.source.to_string_lossy().into_owned(),
        destination: item.destination.to_string_lossy().into_owned(),
        source_hash,
        installed_hash,
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
    })
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
        let err = create(&item, &mut state, None, force_fallback).unwrap_err();
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
            create(&item, &mut state, None, force_fallback).unwrap(),
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
