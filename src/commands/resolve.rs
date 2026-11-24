//! `resolve <TARGET> [--json]` looks up the canonical skill directory or file.
//! TARGET is a path if it is absolute, starts with '.' or '~', or contains a
//! path separator ('/' or, on Windows, '\'). Other targets are skill names.
//!
//! Exit codes: 0 resolved (also `state: "unregistered"` for a draft directory
//! under the registry's skills/), 1 unresolved or unsafe path, 2 unknown skill
//! name, usage, or configuration error.

use std::ffi::OsStr;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;

use serde::Serialize;
use serde_json::json;

use crate::cli::ResolveArgs;
use crate::drive::link::{self, EntryKind};
use crate::drive::state::{InstallRecord, State};
use crate::platform::paths;
use crate::registry::manifest::{InstallMode, Manifest};
use crate::registry::root;
use crate::{Error, Result};

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum ResolutionState {
    Registered,
    Unregistered,
    Foreign,
    Unmanaged,
    Ambiguous,
    Unsafe,
}

struct Resolution {
    state: ResolutionState,
    skill: Option<String>,
    registered: Option<bool>,
    canonical_dir: Option<PathBuf>,
    canonical_path: Option<PathBuf>,
    matched_host: Option<String>,
    mode: Option<InstallMode>,
    reason: Option<String>,
}

impl Resolution {
    fn resolved(
        state: ResolutionState,
        name: String,
        registered: bool,
        dir: PathBuf,
        relative: &Path,
    ) -> Self {
        let mut path = dir.clone();
        path.extend(relative.components());
        Self {
            state,
            skill: Some(name),
            registered: Some(registered),
            canonical_dir: Some(dir),
            canonical_path: Some(path),
            matched_host: None,
            mode: None,
            reason: None,
        }
    }

    fn unresolved(state: ResolutionState, reason: impl Into<String>) -> Self {
        Self {
            state,
            skill: None,
            registered: None,
            canonical_dir: None,
            canonical_path: None,
            matched_host: None,
            mode: None,
            reason: Some(reason.into()),
        }
    }

    fn at_entry(mut self, entry: &Entry, record: Option<&InstallRecord>) -> Self {
        self.skill.get_or_insert_with(|| {
            record.map_or_else(|| entry.name.clone(), |record| record.skill.clone())
        });
        self.matched_host.clone_from(&entry.host);
        self.mode = record.map(|record| record.mode);
        self
    }
}

pub fn run(args: &ResolveArgs) -> ExitCode {
    let (manifest, registry) = match root::load_manifest_resolved(&Default::default(), false) {
        Ok(loaded) => loaded,
        Err(error) => return super::config_error(error),
    };
    let target = &args.target;
    let resolved = if Path::new(target).is_absolute()
        || target.starts_with(['.', '~'])
        || target.contains(std::path::is_separator)
    {
        resolve_path(&manifest, target)
    } else if manifest.skills.contains_key(target) {
        Ok(Resolution::resolved(
            ResolutionState::Registered,
            target.clone(),
            true,
            manifest.skill_dir(target),
            Path::new(""),
        ))
    } else {
        Err(Error::Manifest(format!("unknown skill: {target}")))
    };
    let resolved = match resolved {
        Ok(resolved) => resolved,
        Err(error) => return super::config_error(error),
    };
    let agents = manifest.root.join("AGENTS.md");
    let refusal = registry.write_refusal();
    if args.json {
        let payload = json!({
            "skill": resolved.skill,
            "state": resolved.state,
            "registered": resolved.registered,
            "registry": registry.meta(),
            "canonical_dir": resolved.canonical_dir.as_deref().map(paths::display_path),
            "canonical_path": resolved.canonical_path.as_deref().map(paths::display_path),
            "matched_host": resolved.matched_host,
            "mode": resolved.mode,
            "reason": resolved.reason,
            "write_refusal": refusal,
            "registry_agents_md": agents.is_file().then(|| paths::display_path(&agents)),
        });
        println!("{payload:#}");
    } else {
        if let Some(path) = &resolved.canonical_path {
            println!("{}", paths::display_path(path));
            if matches!(resolved.state, ResolutionState::Unregistered) {
                println!(
                    "Unregistered skill: {}",
                    resolved.skill.as_deref().unwrap_or_default()
                );
            }
        } else if let Some(reason) = &resolved.reason {
            println!("{reason}");
        }
        if let Some(refusal) = refusal {
            println!("{refusal}");
        }
    }
    ExitCode::from(u8::from(resolved.reason.is_some()))
}

struct Entry {
    name: String,
    path: PathBuf,
    relative: PathBuf,
    host: Option<String>,
}

impl Entry {
    fn find(root: &Path, path: &Path, host: Option<&str>) -> Option<Self> {
        let relative = strip_prefix(path, root)?;
        let mut components = relative.components();
        let Component::Normal(name) = components.next()? else {
            return None;
        };
        Some(Self {
            name: name.to_string_lossy().into_owned(),
            path: root.join(name),
            relative: components.as_path().to_owned(),
            host: host.map(str::to_owned),
        })
    }
}

fn resolve_path(manifest: &Manifest, input: &str) -> Result<Resolution> {
    let absolute = absolute_input(Path::new(input))?;
    let mut before_parent = Vec::new();
    let path = lexical_path(&absolute, |path| before_parent.push(path.to_owned()));
    let skills_root = lexical_path(&manifest.skills_dir(), |_| {});
    // Target.path is already absolute and expanded by the manifest loader.
    let targets = manifest
        .targets
        .values()
        .map(|target| (target.name.as_str(), lexical_path(&target.path, |_| {})))
        .collect::<Vec<_>>();

    for prefix in &before_parent {
        let entries = Entry::find(&skills_root, prefix, None).into_iter().chain(
            targets
                .iter()
                .filter_map(|(host, root)| Entry::find(root, prefix, Some(host))),
        );
        for entry in entries {
            if entry.relative.as_os_str().is_empty() || escapes_entry(&entry.path, &entry.relative)?
            {
                let resolution = Resolution::unresolved(
                    ResolutionState::Unsafe,
                    "path traversal escapes the skill entry",
                );
                return if entry.host.is_some() {
                    let state = State::load(None)?;
                    Ok(resolution.at_entry(&entry, matching_records(&state, &entry).next()))
                } else {
                    Ok(resolution.at_entry(&entry, None))
                };
            }
        }
    }

    if let Some(entry) = Entry::find(&skills_root, &path, None) {
        return resolve_canonical(manifest, &entry);
    }
    let mut entries = targets
        .iter()
        .filter_map(|(host, root)| Entry::find(root, &path, Some(host)))
        .collect::<Vec<_>>();
    if !entries.is_empty() {
        // Prefer the most specific host root when records agree on the source.
        entries.sort_by_key(|entry| std::cmp::Reverse(entry.path.components().count()));
        return resolve_host(manifest, &entries);
    }
    if let Ok(root) = fs::canonicalize(&skills_root) {
        let root = path_spelling(&root);
        // A parent step may have erased an alias to the skill directory.
        for prefix in before_parent {
            if let Ok(prefix) = fs::canonicalize(prefix) {
                if let Some(entry) = Entry::find(&root, &path_spelling(&prefix), None) {
                    if entry.relative.as_os_str().is_empty() {
                        return Ok(Resolution::unresolved(
                            ResolutionState::Unsafe,
                            "path traversal escapes the skill entry",
                        )
                        .at_entry(&entry, None));
                    }
                }
            }
        }
        if let Ok(path) = fs::canonicalize(path) {
            if let Some(entry) = Entry::find(&root, &path_spelling(&path), None) {
                return resolve_canonical(manifest, &entry);
            }
        }
    }
    Ok(Resolution::unresolved(
        ResolutionState::Unmanaged,
        "path is outside any skill entry in the registry or configured hosts",
    ))
}

fn resolve_canonical(manifest: &Manifest, entry: &Entry) -> Result<Resolution> {
    let registered = manifest
        .skills
        .keys()
        .find(|name| same_component(OsStr::new(name), OsStr::new(&entry.name)));
    let dir = manifest.skill_dir(registered.unwrap_or(&entry.name));
    let kind = link::entry_kind(&dir)?;
    if matches!(kind, EntryKind::Link(_)) || escapes_entry(&dir, &entry.relative)? {
        return Ok(Resolution::unresolved(
            ResolutionState::Unsafe,
            "a link escapes the canonical skill entry",
        )
        .at_entry(entry, None));
    }
    if registered.is_none() && kind != EntryKind::Dir {
        return Ok(Resolution::unresolved(
            ResolutionState::Unmanaged,
            "path is outside any registered skill or draft directory",
        ));
    }
    Ok(Resolution::resolved(
        if registered.is_some() {
            ResolutionState::Registered
        } else {
            ResolutionState::Unregistered
        },
        registered.unwrap_or(&entry.name).clone(),
        registered.is_some(),
        dir,
        &entry.relative,
    ))
}

fn resolve_host(manifest: &Manifest, entries: &[Entry]) -> Result<Resolution> {
    let state = State::load(None)?;
    let mut matches = entries
        .iter()
        .flat_map(|entry| matching_records(&state, entry).map(move |record| (entry, record)));
    let Some((entry, record)) = matches.next() else {
        let entry = &entries[0];
        let kind = link::entry_kind(&entry.path)?;
        let unsafe_link = (matches!(kind, EntryKind::Link(_))
            && !link::is_managed_link(&entry.path, &manifest.skill_dir(&entry.name)))
            || escapes_entry(&entry.path, &entry.relative)?;
        let resolution = if unsafe_link {
            Resolution::unresolved(
                ResolutionState::Unsafe,
                "host link leaves the canonical skill entry",
            )
        } else {
            Resolution::unresolved(
                ResolutionState::Unmanaged,
                "host entry has no install record",
            )
        };
        return Ok(resolution.at_entry(entry, None));
    };
    let source = Path::new(&record.source);
    let skills_root = manifest.skills_dir();
    let owned = record.belongs_to(&skills_root);
    if matches.any(|(_, other)| {
        let other_source = Path::new(&other.source);
        !same_path(source, other_source)
            && !(owned
                && other.belongs_to(&skills_root)
                && source
                    .file_name()
                    .zip(other_source.file_name())
                    .is_some_and(|(a, b)| same_component(a, b)))
    }) {
        return Ok(Resolution::unresolved(
            ResolutionState::Ambiguous,
            "host paths have install records for different canonical skills",
        ));
    }
    if !owned {
        return Ok(
            Resolution::unresolved(ResolutionState::Foreign, record.foreign_reason())
                .at_entry(entry, Some(record)),
        );
    }
    let dir = skills_root.join(
        source
            .file_name()
            .expect("an owned source has an entry name"),
    );
    let kind = link::entry_kind(&entry.path)?;
    if (matches!(kind, EntryKind::Link(_))
        && (record.mode != InstallMode::Link || !link::is_managed_link(&entry.path, &dir)))
        || escapes_entry(&entry.path, &entry.relative)?
        || escapes_entry(&dir, &entry.relative)?
    {
        return Ok(Resolution::unresolved(
            ResolutionState::Unsafe,
            "host link does not stay within the managed skill entry",
        )
        .at_entry(entry, Some(record)));
    }
    let registered = manifest.skills.contains_key(&record.skill);
    let resolution = Resolution::resolved(
        ResolutionState::Registered,
        record.skill.clone(),
        registered,
        dir,
        &entry.relative,
    )
    .at_entry(entry, Some(record));
    if record.mode == InstallMode::Copy
        && !entry.relative.as_os_str().is_empty()
        && resolution
            .canonical_path
            .as_ref()
            .is_some_and(|path| !path.exists())
    {
        let reason = if entry.path.join(&entry.relative).exists() {
            "file exists only in the host copy of a managed skill"
        } else {
            "relative path is missing from the canonical skill and its managed copy"
        };
        let mut unresolved = Resolution::unresolved(ResolutionState::Unmanaged, reason)
            .at_entry(entry, Some(record));
        unresolved.registered = Some(registered);
        return Ok(unresolved);
    }
    Ok(resolution)
}

fn matching_records<'a>(
    state: &'a State,
    entry: &'a Entry,
) -> impl Iterator<Item = &'a InstallRecord> {
    state.records().filter(|record| {
        entry.host.as_deref() == Some(record.host.as_str())
            && same_path(Path::new(&record.destination), &entry.path)
    })
}

/// Visit each requested component, including a link followed by a missing
/// file, so a missing leaf cannot hide a link that leaves the entry.
fn escapes_entry(entry: &Path, relative: &Path) -> Result<bool> {
    let mut prefix = entry.to_owned();
    let mut components = relative.components().peekable();
    while let Some(component) = components.next() {
        prefix.push(component.as_os_str());
        let kind = match link::entry_kind(&prefix) {
            Err(Error::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotADirectory => {
                break;
            }
            result => result?,
        };
        match kind {
            EntryKind::Link(_) => {
                let Ok(target) = fs::canonicalize(&prefix) else {
                    return Ok(true);
                };
                let root = fs::canonicalize(entry).map_err(|error| Error::io(entry, error))?;
                let target = path_spelling(&target);
                let root = path_spelling(&root);
                if strip_prefix(&target, &root).is_none() {
                    return Ok(true);
                }
                if components.peek().is_some() && !target.is_dir() {
                    break;
                }
            }
            EntryKind::Dir => {}
            EntryKind::File | EntryKind::Missing => break,
        }
    }
    Ok(false)
}

fn absolute_input(path: &Path) -> Result<PathBuf> {
    let path = path_spelling(path);
    let path = match path.strip_prefix("~") {
        Ok(suffix) => path_spelling(&paths::home_dir()).join(suffix),
        Err(_) => path,
    };
    if path.is_absolute() {
        return Ok(path);
    }
    #[cfg(windows)]
    if let Some(Component::Prefix(prefix)) = path.components().next() {
        let drive = Path::new(prefix.as_os_str());
        let base = std::path::absolute(drive).map_err(|error| Error::io(drive, error))?;
        return Ok(path_spelling(&base).join(path.strip_prefix(drive).expect("drive prefix")));
    }
    let cwd = std::env::current_dir().map_err(|error| Error::io(".", error))?;
    Ok(path_spelling(&cwd).join(path))
}

fn path_spelling(path: &Path) -> PathBuf {
    #[cfg(windows)]
    return PathBuf::from(paths::display_path(path));
    #[cfg(not(windows))]
    return path.to_owned();
}

fn lexical_path(path: &Path, mut before_parent: impl FnMut(&Path)) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path_spelling(path).components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if matches!(
                    normalized.components().next_back(),
                    Some(Component::Normal(_))
                ) {
                    before_parent(&normalized);
                    normalized.pop();
                }
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

fn strip_prefix(path: &Path, root: &Path) -> Option<PathBuf> {
    let mut path = path.components();
    for part in root.components() {
        if !same_component(path.next()?.as_os_str(), part.as_os_str()) {
            return None;
        }
    }
    Some(path.as_path().to_owned())
}

fn same_component(a: &OsStr, b: &OsStr) -> bool {
    a == b
        || (cfg!(windows)
            && a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase())
}

fn same_path(a: &Path, b: &Path) -> bool {
    let a = lexical_path(a, |_| {});
    let b = lexical_path(b, |_| {});
    strip_prefix(&a, &b).is_some_and(|relative| relative.as_os_str().is_empty())
}
