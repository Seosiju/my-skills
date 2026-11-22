//! Planning: what each (skill, host) pair needs. Integrator. Spec §6.2–§6.6,
//! decisions D3 (reconcile with prune) and I3/I5/I7.
//!
//! Planning reads the filesystem but never writes it. `install::apply`
//! executes the plan.

use std::path::PathBuf;

use crate::hash::hash_directory;
use crate::link::{self, EntryKind};
use crate::manifest::{InstallMode, Manifest, Skill};
use crate::state::{InstallRecord, State};
use crate::{Error, Result};

/// Read-only state of one connection (spec §6.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Fresh,
    Stale,
    Drifted,
    Conflict,
    Broken,
    Missing,
    Unmanaged,
    Unsupported,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Status::Fresh => "FRESH",
            Status::Stale => "STALE",
            Status::Drifted => "DRIFTED",
            Status::Conflict => "CONFLICT",
            Status::Broken => "BROKEN",
            Status::Missing => "MISSING",
            Status::Unmanaged => "UNMANAGED",
            Status::Unsupported => "UNSUPPORTED",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Create,
    Update,
    Noop,
    Remove,
    Convert,
    BlockConflict,
    BlockDrift,
    Conflict,
    SkipUnsupported,
    NotManaged,
}

impl Action {
    pub fn label(self) -> &'static str {
        match self {
            Action::Create => "CREATE",
            Action::Update => "UPDATE",
            Action::Noop => "NOOP",
            Action::Remove => "REMOVE",
            Action::Convert => "CONVERT",
            Action::BlockConflict => "BLOCK_CONFLICT",
            Action::BlockDrift => "BLOCK_DRIFT",
            Action::Conflict => "CONFLICT",
            Action::SkipUnsupported => "SKIP_UNSUPPORTED",
            Action::NotManaged => "NOT_MANAGED",
        }
    }

    pub fn is_block(self) -> bool {
        matches!(
            self,
            Action::BlockConflict | Action::BlockDrift | Action::Conflict
        )
    }

    pub fn writes(self) -> bool {
        matches!(
            self,
            Action::Create | Action::Update | Action::Remove | Action::Convert
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanItem {
    pub skill: String,
    pub host: String,
    /// Canonical skill directory.
    pub source: PathBuf,
    pub destination: PathBuf,
    pub action: Action,
    pub reason: String,
    /// Mode the connection has (or gets, for Create / Convert).
    pub mode: InstallMode,
    /// Canonical hash when planning computed it (copy paths only).
    pub source_hash: Option<String>,
}

impl PlanItem {
    fn new(
        skill: &str,
        host: &str,
        source: PathBuf,
        destination: PathBuf,
        action: Action,
        reason: &str,
        mode: InstallMode,
    ) -> Self {
        PlanItem {
            skill: skill.to_string(),
            host: host.to_string(),
            source,
            destination,
            action,
            reason: reason.to_string(),
            mode,
            source_hash: None,
        }
    }

    fn with_hash(mut self, hash: Option<String>) -> Self {
        self.source_hash = hash;
        self
    }
}

/// `target.path / skill`.
pub fn destination(manifest: &Manifest, host: &str, skill: &str) -> Result<PathBuf> {
    manifest
        .targets
        .get(host)
        .map(|t| t.path.join(skill))
        .ok_or_else(|| Error::Manifest(format!("unknown host: {host}")))
}

fn host_supported(skill: &Skill, host: &str) -> bool {
    skill.hosts.is_empty() || skill.hosts.iter().any(|h| h == host)
}

/// The drive's desired state (D3): the skill is enabled, the target is
/// enabled, and the skill lists the host (or lists none).
pub fn wants(manifest: &Manifest, skill: &Skill, host: &str) -> bool {
    skill.enabled
        && manifest.targets.get(host).is_some_and(|t| t.enabled)
        && host_supported(skill, host)
}

/// Read-only status of one (skill, host) pair (spec §6.2).
pub fn status_of(manifest: &Manifest, skill: &Skill, host: &str, state: &State) -> Result<Status> {
    if !host_supported(skill, host) {
        return Ok(Status::Unsupported);
    }
    let source = manifest.skill_dir(&skill.name);
    let dest = destination(manifest, host, &skill.name)?;
    let kind = link::entry_kind(&dest)?;
    let Some(record) = state.get(&skill.name, host) else {
        return Ok(if kind == EntryKind::Missing {
            Status::Missing
        } else {
            Status::Unmanaged
        });
    };
    match record.mode {
        InstallMode::Link => Ok(link_status(&dest, &source, kind)),
        InstallMode::Copy => copy_status(&dest, &source, kind, record),
    }
}

fn link_status(dest: &std::path::Path, source: &std::path::Path, kind: EntryKind) -> Status {
    match kind {
        EntryKind::Missing => Status::Missing,
        EntryKind::Link(_) if link::is_managed_link(dest, source) => {
            if source.is_dir() {
                Status::Fresh
            } else {
                Status::Broken
            }
        }
        _ => Status::Drifted,
    }
}

fn copy_status(
    dest: &std::path::Path,
    source: &std::path::Path,
    kind: EntryKind,
    record: &InstallRecord,
) -> Result<Status> {
    match kind {
        EntryKind::Missing => return Ok(Status::Missing),
        // A copy replaced by a link or a file was changed outside my-skills.
        EntryKind::Link(_) | EntryKind::File => return Ok(Status::Drifted),
        EntryKind::Dir => {}
    }
    let install_modified = hash_directory(dest)? != record.installed_hash;
    let source_changed = !source.is_dir() || hash_directory(source)? != record.source_hash;
    Ok(match (install_modified, source_changed) {
        (true, true) => Status::Conflict,
        (true, false) => Status::Drifted,
        (false, true) => Status::Stale,
        (false, false) => Status::Fresh,
    })
}

/// Plan connecting one skill to one host. An existing record keeps its own
/// mode; a new connection uses `requested_mode` (D1).
pub fn plan_install_one(
    manifest: &Manifest,
    skill: &Skill,
    host: &str,
    state: &State,
    requested_mode: InstallMode,
) -> Result<PlanItem> {
    let source = manifest.skill_dir(&skill.name);
    let dest = destination(manifest, host, &skill.name)?;
    let record = state.get(&skill.name, host);
    let mode = record.map_or(requested_mode, |r| r.mode);
    let item = |action, reason: &str, mode| {
        PlanItem::new(
            &skill.name,
            host,
            source.clone(),
            dest.clone(),
            action,
            reason,
            mode,
        )
    };

    if !host_supported(skill, host) {
        return Ok(item(
            Action::SkipUnsupported,
            &format!("{host} not in skill.hosts"),
            mode,
        ));
    }

    let kind = link::entry_kind(&dest)?;
    if kind == EntryKind::Missing {
        return Ok(item(Action::Create, "destination missing", mode));
    }
    let Some(record) = record else {
        return Ok(item(
            Action::BlockConflict,
            "destination exists and is not managed by my-skills",
            mode,
        ));
    };

    match record.mode {
        InstallMode::Link => Ok(match link_status(&dest, &source, kind) {
            Status::Fresh => item(Action::Noop, "up to date (linked)", InstallMode::Link),
            Status::Broken => item(
                Action::BlockDrift,
                "canonical skill directory is missing (broken link)",
                InstallMode::Link,
            ),
            _ => item(
                Action::BlockDrift,
                "managed link was replaced locally",
                InstallMode::Link,
            ),
        }),
        InstallMode::Copy => {
            let status = copy_status(&dest, &source, kind, record)?;
            let hash = if source.is_dir() {
                Some(hash_directory(&source)?)
            } else {
                None
            };
            let planned = match status {
                Status::Conflict => item(
                    Action::Conflict,
                    "canonical and installed copy both changed (no auto-merge)",
                    InstallMode::Copy,
                ),
                Status::Drifted => item(
                    Action::BlockDrift,
                    "installed copy was modified locally",
                    InstallMode::Copy,
                ),
                Status::Stale if hash.is_none() => item(
                    Action::BlockDrift,
                    "canonical skill directory is missing",
                    InstallMode::Copy,
                ),
                Status::Stale => item(Action::Update, "canonical changed", InstallMode::Copy),
                _ => item(Action::Noop, "up to date", InstallMode::Copy),
            };
            Ok(planned.with_hash(hash))
        }
    }
}

/// Plan removing one managed connection. Uses the recorded destination.
pub fn plan_uninstall_one(
    manifest: &Manifest,
    skill_name: &str,
    host: &str,
    state: &State,
) -> Result<PlanItem> {
    let source = manifest.skill_dir(skill_name);
    let Some(record) = state.get(skill_name, host) else {
        let dest = destination(manifest, host, skill_name)?;
        return Ok(PlanItem::new(
            skill_name,
            host,
            source,
            dest,
            Action::NotManaged,
            "not recorded as managed",
            InstallMode::Link,
        ));
    };
    let dest = PathBuf::from(&record.destination);
    let kind = link::entry_kind(&dest)?;
    let item = |action, reason: &str| {
        PlanItem::new(
            skill_name,
            host,
            source.clone(),
            dest.clone(),
            action,
            reason,
            record.mode,
        )
    };
    Ok(match record.mode {
        InstallMode::Link => match kind {
            EntryKind::Missing => item(Action::Remove, "managed link (already gone)"),
            EntryKind::Link(_) if link::is_managed_link(&dest, &source) => {
                item(Action::Remove, "managed link")
            }
            _ => item(Action::BlockDrift, "managed link was replaced locally"),
        },
        InstallMode::Copy => match kind {
            EntryKind::Missing => item(Action::Remove, "managed install (already gone)"),
            EntryKind::Dir if hash_directory(&dest)? == record.installed_hash => {
                item(Action::Remove, "managed install")
            }
            _ => item(Action::BlockDrift, "installed copy was modified locally"),
        },
    })
}

/// What `sync` should look at.
pub struct SyncScope<'a> {
    pub skills: &'a [&'a Skill],
    pub hosts: &'a [String],
    /// Also prune records whose skill is no longer in the manifest. True when
    /// no skill was named on the command line.
    pub include_orphans: bool,
    /// D3 prune; false with `--no-prune`.
    pub prune: bool,
}

/// Reconcile (D3): connect what the manifest wants, prune managed
/// connections it no longer wants. Disabled targets are never touched (I7).
pub fn plan_sync(
    manifest: &Manifest,
    state: &State,
    scope: &SyncScope<'_>,
    requested_mode: InstallMode,
) -> Result<Vec<PlanItem>> {
    let mut items = Vec::new();
    for host in scope.hosts {
        let target_enabled = manifest.targets.get(host).is_some_and(|t| t.enabled);
        if !target_enabled {
            continue;
        }
        for skill in scope.skills {
            if wants(manifest, skill, host) {
                items.push(plan_install_one(
                    manifest,
                    skill,
                    host,
                    state,
                    requested_mode,
                )?);
            } else if scope.prune && state.get(&skill.name, host).is_some() {
                let mut item = plan_uninstall_one(manifest, &skill.name, host, state)?;
                if item.action == Action::Remove {
                    item.reason = format!("{} (no longer wanted)", item.reason);
                }
                items.push(item);
            }
        }
        if scope.prune && scope.include_orphans {
            for record in state.records() {
                if &record.host == host && !manifest.skills.contains_key(&record.skill) {
                    let mut item = plan_uninstall_one(manifest, &record.skill, host, state)?;
                    if item.action == Action::Remove {
                        item.reason = format!("{} (skill no longer registered)", item.reason);
                    }
                    items.push(item);
                }
            }
        }
    }
    Ok(items)
}

/// Plan switching existing connections to `to` (spec §6.6).
pub fn plan_convert(
    manifest: &Manifest,
    state: &State,
    skills: &[&Skill],
    hosts: &[String],
    to: InstallMode,
) -> Result<Vec<PlanItem>> {
    let mut items = Vec::new();
    for skill in skills {
        for host in hosts {
            let Some(record) = state.get(&skill.name, host) else {
                continue;
            };
            let source = manifest.skill_dir(&skill.name);
            let dest = PathBuf::from(&record.destination);
            let expected = destination(manifest, host, &skill.name)?;
            let status = status_of(manifest, skill, host, state)?;
            let item = |action, reason: &str| {
                PlanItem::new(
                    &skill.name,
                    host,
                    source.clone(),
                    dest.clone(),
                    action,
                    reason,
                    to,
                )
            };
            let planned = if record.mode == to {
                item(Action::Noop, "already in the requested mode")
            } else if dest != expected {
                // status_of judged the manifest's current path; converting a
                // connection recorded elsewhere would act on a different entry.
                item(
                    Action::BlockDrift,
                    "recorded destination differs from the manifest target; run sync first",
                )
            } else if !source.is_dir() {
                item(Action::BlockDrift, "canonical skill directory is missing")
            } else {
                match (to, status) {
                    (InstallMode::Link, Status::Fresh | Status::Stale) => {
                        item(Action::Convert, "copy has no local edits")
                    }
                    (InstallMode::Copy, Status::Fresh) => item(Action::Convert, "link is healthy"),
                    (_, Status::Conflict) => item(
                        Action::Conflict,
                        "canonical and installed copy both changed; resolve first",
                    ),
                    (_, Status::Missing) => item(
                        Action::BlockDrift,
                        "connection is missing; run sync instead",
                    ),
                    (_, other) => item(
                        Action::BlockDrift,
                        &format!("cannot convert a {} connection", other.label()),
                    ),
                }
            };
            items.push(planned);
        }
    }
    Ok(items)
}
