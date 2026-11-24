//! Planning: what each (skill, host) pair needs. Integrator. Spec §6.2–§6.6,
//! decisions D3 (reconcile with prune) and I3/I5/I7.
//!
//! Planning reads the filesystem but never writes it. `apply::apply`
//! executes the plan.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::drive::hash::hash_directory;
use crate::drive::link::{self, EntryKind};
use crate::drive::state::{InstallRecord, State};
use crate::registry::manifest::{InstallMode, Manifest, Skill};
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
    Foreign,
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
            Status::Foreign => "FOREIGN",
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
    BlockForeign,
    Conflict,
    /// An unselected connection shares this entry and would be invalidated.
    SharedDestination,
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
            Action::BlockForeign => "BLOCK_FOREIGN",
            Action::Conflict => "CONFLICT",
            Action::SharedDestination => "SHARED_DESTINATION",
            Action::SkipUnsupported => "SKIP_UNSUPPORTED",
            Action::NotManaged => "NOT_MANAGED",
        }
    }

    pub fn is_block(self) -> bool {
        matches!(
            self,
            Action::BlockConflict
                | Action::BlockDrift
                | Action::BlockForeign
                | Action::Conflict
                | Action::SharedDestination
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

    fn block_foreign(source: PathBuf, record: &InstallRecord) -> Self {
        Self::new(
            &record.skill,
            &record.host,
            source,
            PathBuf::from(&record.destination),
            Action::BlockForeign,
            &record.foreign_reason(),
            record.mode,
        )
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
    let record = state.get(&skill.name, host);
    if record.is_some_and(|record| !record.belongs_to(&manifest.skills_dir())) {
        return Ok(Status::Foreign);
    }
    let source = manifest.skill_dir(&skill.name);
    let dest = destination(manifest, host, &skill.name)?;
    connection_status(&dest, &source, record)
}

fn connection_status(dest: &Path, source: &Path, record: Option<&InstallRecord>) -> Result<Status> {
    let kind = link::entry_kind(dest)?;
    let Some(record) = record else {
        return Ok(if kind == EntryKind::Missing {
            Status::Missing
        } else {
            Status::Unmanaged
        });
    };
    match record.mode {
        InstallMode::Link => Ok(link_status(dest, source, kind)),
        InstallMode::Copy => copy_status(dest, source, kind, record),
    }
}

fn link_status(dest: &Path, source: &Path, kind: EntryKind) -> Status {
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

fn copy_source_hash(source: &Path) -> Result<String> {
    // Copying follows a linked skill root while refusing links inside the
    // tree. Hash the same root for freshness checks.
    if matches!(link::entry_kind(source)?, EntryKind::Link(_)) {
        let resolved = fs::canonicalize(source).map_err(|err| Error::io(source, err))?;
        return hash_directory(&resolved);
    }
    hash_directory(source)
}

fn copy_status(
    dest: &Path,
    source: &Path,
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
    let source_changed = !source.is_dir() || copy_source_hash(source)? != record.source_hash;
    Ok(match (install_modified, source_changed) {
        (true, true) => Status::Conflict,
        (true, false) => Status::Drifted,
        (false, true) => Status::Stale,
        (false, false) => Status::Fresh,
    })
}

/// Locate the host entry without following the final link. Different live
/// connections may point to the same source and still have distinct entries.
pub(crate) fn physical_destination(path: &Path) -> Result<PathBuf> {
    for ancestor in path
        .ancestors()
        .skip(1)
        .filter(|p| !p.as_os_str().is_empty())
    {
        match fs::canonicalize(ancestor) {
            Ok(real) => {
                let suffix = path
                    .strip_prefix(ancestor)
                    .expect("ancestor is a path prefix");
                let entry = real.join(suffix);
                #[cfg(windows)]
                let entry = PathBuf::from(entry.to_string_lossy().to_lowercase());
                return Ok(entry);
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(Error::io(ancestor, err)),
        }
    }
    Err(Error::Manifest(format!(
        "no existing parent for {}",
        path.display()
    )))
}

#[derive(Default)]
pub(crate) struct SharedDestinations {
    pub(crate) groups: BTreeMap<usize, Vec<usize>>,
    pub(crate) incompatible: bool,
}

pub(crate) fn group_destinations(
    manifest: &Manifest,
    state: &State,
    items: &mut [PlanItem],
) -> Result<SharedDestinations> {
    let mut indices: BTreeMap<PathBuf, Vec<usize>> = BTreeMap::new();
    for (index, item) in items.iter().enumerate() {
        if !matches!(item.action, Action::SkipUnsupported | Action::NotManaged) {
            indices
                .entry(physical_destination(&item.destination)?)
                .or_default()
                .push(index);
        }
    }
    let mut result = SharedDestinations::default();
    for group in indices.into_values().filter(|group| group.len() > 1) {
        let first = &items[group[0]];
        let same_items = group.iter().all(|&idx| {
            let item = &items[idx];
            item.skill == first.skill && item.source == first.source && item.mode == first.mode
        });
        let same_action = group.iter().all(|&idx| items[idx].action == first.action);
        if same_action && first.action == Action::Remove {
            continue;
        }
        // One peer is pruned while the others keep (or refresh) the shared
        // entry: apply drops only the pruned peer's record.
        let keeps = |item: &PlanItem| matches!(item.action, Action::Noop | Action::Update);
        if same_items
            && group.iter().any(|&idx| items[idx].action == Action::Remove)
            && group.iter().any(|&idx| keeps(&items[idx]))
            && group
                .iter()
                .all(|&idx| items[idx].action == Action::Remove || keeps(&items[idx]))
        {
            continue;
        }
        if same_items
            && same_action
            && matches!(
                first.action,
                Action::Create | Action::Update | Action::Convert
            )
        {
            result.groups.insert(group[0], group[1..].to_vec());
            continue;
        }
        // A managed peer proves ownership and the clean installed digest for
        // an alias left unrecorded by a previous partial install.
        let managed_copy = group
            .iter()
            .copied()
            .find(|&idx| matches!(items[idx].action, Action::Update | Action::Noop));
        let unrecorded = |item: &PlanItem| {
            item.action == Action::BlockConflict
                && item.reason == "destination exists and is not managed by my-skills"
        };
        if same_items
            && first.mode == InstallMode::Copy
            && let Some(primary) = managed_copy
            && group.iter().all(|&idx| {
                matches!(items[idx].action, Action::Update | Action::Noop)
                    || unrecorded(&items[idx])
            })
        {
            if !same_action {
                for &idx in &group {
                    items[idx].action = Action::Update;
                    items[idx].reason = "reconcile shared copy destination".into();
                }
                result.groups.insert(
                    primary,
                    group
                        .iter()
                        .copied()
                        .filter(|&idx| idx != primary)
                        .collect(),
                );
            }
            continue;
        }
        if !same_items || group.iter().any(|&idx| items[idx].action.writes()) {
            result.incompatible = true;
            for idx in group {
                items[idx].action = Action::Conflict;
                items[idx].reason =
                    "shared destination has incompatible planned connections".into();
            }
        }
    }
    guard_unselected_peers(manifest, state, items, &mut result)?;
    Ok(result)
}

/// A connection recorded in state but absent from this plan may share a
/// destination with a planned write. Removing, converting, or refreshing that
/// entry would leave the peer's record describing something that is gone, so
/// block those writes before anything changes.
fn guard_unselected_peers(
    manifest: &Manifest,
    state: &State,
    items: &mut [PlanItem],
    result: &mut SharedDestinations,
) -> Result<()> {
    let skills_dir = manifest.skills_dir();
    let peers: Vec<(&InstallRecord, PathBuf)> = state
        .records()
        .filter(|record| {
            record.belongs_to(&skills_dir)
                && !items
                    .iter()
                    .any(|item| item.skill == record.skill && item.host == record.host)
        })
        .filter_map(|record| {
            let physical = physical_destination(Path::new(&record.destination)).ok()?;
            Some((record, physical))
        })
        .collect();
    if peers.is_empty() {
        return Ok(());
    }
    for item in items.iter_mut() {
        if !matches!(
            item.action,
            Action::Remove | Action::Update | Action::Convert
        ) {
            continue;
        }
        // Nothing to invalidate when the entry is already gone.
        if item.action == Action::Remove
            && link::entry_kind(&item.destination)? == EntryKind::Missing
        {
            continue;
        }
        let Ok(physical) = physical_destination(&item.destination) else {
            continue;
        };
        let Some((peer, _)) = peers.iter().find(|(_, other)| *other == physical) else {
            continue;
        };
        let remedy = if manifest.targets.get(&peer.host).is_some_and(|t| t.enabled) {
            "select every host that shares it (omit --host) or handle that host first"
        } else {
            "enable that host and include both hosts"
        };
        item.reason = format!(
            "destination is shared with {} -> {}, which this command does not include; {remedy}",
            peer.skill, peer.host
        );
        item.action = Action::SharedDestination;
        result.incompatible = true;
    }
    Ok(())
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
    if let Some(record) = record.filter(|record| !record.belongs_to(&manifest.skills_dir())) {
        return Ok(PlanItem::block_foreign(source, record));
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
                Some(copy_source_hash(&source)?)
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
    if !record.belongs_to(&manifest.skills_dir()) {
        return Ok(PlanItem::block_foreign(source, record));
    }
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
///
/// When `requested_mode` is `Some(mode)`, it overrides the manifest mode for
/// new connections; otherwise each host uses `manifest.effective_install_mode(host)`.
pub fn plan_sync(
    manifest: &Manifest,
    state: &State,
    scope: &SyncScope<'_>,
    requested_mode: Option<InstallMode>,
) -> Result<Vec<PlanItem>> {
    let mut items = Vec::new();
    for host in scope.hosts {
        let target_enabled = manifest.targets.get(host).is_some_and(|t| t.enabled);
        if !target_enabled {
            continue;
        }
        let host_mode = requested_mode.unwrap_or_else(|| manifest.effective_install_mode(host));
        for skill in scope.skills {
            if wants(manifest, skill, host) {
                items.push(plan_install_one(manifest, skill, host, state, host_mode)?);
            } else if scope.prune && state.get(&skill.name, host).is_some() {
                items.extend(plan_prune(
                    manifest,
                    &skill.name,
                    host,
                    state,
                    "no longer wanted",
                )?);
            }
        }
        if scope.prune && scope.include_orphans {
            for record in state.records() {
                if &record.host == host && !manifest.skills.contains_key(&record.skill) {
                    items.extend(plan_prune(
                        manifest,
                        &record.skill,
                        host,
                        state,
                        "skill no longer registered",
                    )?);
                }
            }
        }
    }
    Ok(items)
}

fn plan_prune(
    manifest: &Manifest,
    skill: &str,
    host: &str,
    state: &State,
    reason: &str,
) -> Result<Option<PlanItem>> {
    let mut item = plan_uninstall_one(manifest, skill, host, state)?;
    match item.action {
        Action::BlockForeign => return Ok(None),
        Action::Remove => item.reason = format!("{} ({reason})", item.reason),
        _ => {}
    }
    Ok(Some(item))
}

/// Partition hosts for `convert --to <mode>`.
///
/// When `explicit_host` is false (`--host all` or omitted), any host whose
/// `[targets.<host>] install_mode` is set explicitly and differs from `to`
/// is returned in the skipped list rather than the active list. An explicit
/// `--host <name>` is never skipped.
pub fn filter_convert_hosts(
    manifest: &Manifest,
    hosts: &[String],
    to: InstallMode,
    explicit_host: bool,
) -> (Vec<String>, Vec<(String, InstallMode)>) {
    if explicit_host {
        return (hosts.to_vec(), Vec::new());
    }
    let mut active = Vec::new();
    let mut skipped = Vec::new();
    for host in hosts {
        if let Some(mode) = manifest
            .targets
            .get(host)
            .and_then(|target| target.install_mode)
            .filter(|&mode| mode != to)
        {
            skipped.push((host.clone(), mode));
        } else {
            active.push(host.clone());
        }
    }
    (active, skipped)
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
            if !record.belongs_to(&manifest.skills_dir()) {
                items.push(PlanItem::block_foreign(source, record));
                continue;
            }
            let dest = PathBuf::from(&record.destination);
            let expected = destination(manifest, host, &skill.name)?;
            let status = if host_supported(skill, host) {
                connection_status(&expected, &source, Some(record))?
            } else {
                Status::Unsupported
            };
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
                // connection_status judged the manifest's current path; converting a
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
