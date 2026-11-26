//! Shared command runtime: registry root resolution, host and skill
//! selection, write confirmation. Integrator. Mirrors v0.4.1 `cli_runtime.py`
//! and `write_confirmation.py`.
//!
//! Root resolution order (highest first):
//! 1. `--root <PATH>` (never cached),
//! 2. `$MY_SKILLS_ROOT` (an invalid value is an error),
//! 3. `my-skills.toml` in the current directory or a parent; cached only when
//!    a write is requested and no valid active root exists yet,
//! 4. the cached active root (`$XDG_CONFIG_HOME/my-skills/root`).

use std::fs;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use fs4::FileExt;
use serde::Serialize;

use crate::registry::manifest::{self, CliOverrides, MANIFEST_FILE, Manifest, Skill};
use crate::{Error, Result, platform::paths};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootSource {
    Flag,
    Env,
    Cwd,
    Cache,
}

impl RootSource {
    pub fn label(self) -> &'static str {
        match self {
            RootSource::Flag => "flag",
            RootSource::Env => "env",
            RootSource::Cwd => "cwd",
            RootSource::Cache => "cache",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootResolution {
    pub root: PathBuf,
    pub source: RootSource,
    /// The cached active root, if one is valid.
    pub cached: Option<PathBuf>,
    cache: CacheSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RegistryMeta {
    pub path: String,
    pub source: &'static str,
}

impl RootResolution {
    /// True when a real (non-dry-run) command would record `self.root` as the
    /// active registry because no valid cache exists yet.
    pub fn would_cache(&self) -> bool {
        self.source == RootSource::Cwd && self.cached.is_none()
    }

    /// Recheck implicit write eligibility once the caller holds the state lock.
    pub(crate) fn recheck_write(&self) -> Result<()> {
        let current = read_root_cache();
        let refusal = match self.source {
            RootSource::Cwd => current.as_deref().filter(|active| *active != self.root),
            RootSource::Cache if current.as_deref() != Some(self.root.as_path()) => {
                if current.is_none() {
                    note_cache_change(&self.root);
                    return Err(Error::Manifest(
                        "active registry was removed; retry with an explicit registry".into(),
                    ));
                }
                current.as_deref()
            }
            _ => None,
        };
        if let Some(active) = refusal {
            note_cache_change(&self.root);
            return Err(Error::Manifest(write_refusal_message(&self.root, active)));
        }
        Ok(())
    }

    /// Cache only a first cwd discovery, and preserve a newer set-root.
    pub(crate) fn remember(&self) -> bool {
        if self.would_cache() {
            return self.cache.remember(&self.root);
        }
        if matches!(self.source, RootSource::Cwd | RootSource::Cache)
            && let Ok(current) = cache_contents(&root_cache_path())
            && current != self.cache.0
        {
            note_cache_change(&self.root);
        }
        false
    }

    pub fn meta(&self) -> RegistryMeta {
        RegistryMeta {
            path: paths::display_path(&self.root),
            source: self.source.label(),
        }
    }

    /// The error a real write would hit with this resolution: the root came
    /// from the current directory and a different registry is active. A
    /// preview resolves without the write policy, so it asks here to report
    /// the refusal instead of claiming the command would apply.
    pub fn write_refusal(&self) -> Option<String> {
        different_active_root(self).map(|active| write_refusal_message(&self.root, active))
    }
}

fn write_refusal_message(root: &Path, active: &Path) -> String {
    format!(
        "refusing to write to cwd registry {} while the active registry is {}. \
         Run from the active registry, run 'my-skills set-root <path>' to switch, \
         or set MY_SKILLS_ROOT=<path> for one command.",
        paths::display_path(root),
        paths::display_path(active),
    )
}

static EXPLICIT_ROOT: OnceLock<PathBuf> = OnceLock::new();

/// Record the global `--root <PATH>` for this process. `cli::run` calls it
/// once before dispatching.
pub fn set_explicit_root(path: Option<PathBuf>) {
    if let Some(path) = path {
        let _ = EXPLICIT_ROOT.set(path);
    }
}

/// The `--root` given for this process, if any.
pub fn explicit_root() -> Option<&'static Path> {
    EXPLICIT_ROOT.get().map(PathBuf::as_path)
}

pub fn root_cache_path() -> PathBuf {
    paths::config_home().join("my-skills").join("root")
}

fn is_root(path: &Path) -> bool {
    path.join(MANIFEST_FILE).is_file()
}

fn resolve(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct CacheSnapshot(Option<Vec<u8>>);

impl CacheSnapshot {
    pub(crate) fn observe() -> Self {
        Self(cache_contents(&root_cache_path()).ok().flatten())
    }

    pub(crate) fn remember(&self, root: &Path) -> bool {
        write_root_cache(root, Some(self))
    }

    fn active(&self) -> Option<PathBuf> {
        self.0.as_deref().and_then(cached_root)
    }
}

fn note_cache_change(root: &Path) {
    eprintln!(
        "note: active registry cache changed while writing {}; leaving the newer selection alone",
        paths::display_path(root),
    );
}

fn write_root_cache(root: &Path, observed: Option<&CacheSnapshot>) -> bool {
    try_write_root_cache(root, observed).unwrap_or(false)
}

/// Like `write_root_cache`, but reports lock and write failures. Implicit
/// caching is best effort; an explicit `set-root` must not claim success
/// when nothing was saved.
fn try_write_root_cache(root: &Path, observed: Option<&CacheSnapshot>) -> io::Result<bool> {
    if explicit_root().is_some() {
        return Ok(false);
    }
    let cache = root_cache_path();
    let parent = cache.parent().expect("root cache has a parent");
    let write = || -> io::Result<bool> {
        fs::create_dir_all(parent)?;
        // Serialize the conditional update with explicit set-root writes.
        // A separate file lets set-root proceed while a host writer waits.
        let lock = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(cache.with_file_name("root.lock"))?;
        FileExt::lock(&lock)?;
        if let Some(original) = observed
            && let Some(current) = cache_contents(&cache)?
            && original.0.as_deref() != Some(&current)
        {
            note_cache_change(root);
            return Ok(false);
        }
        let mut temp = tempfile::NamedTempFile::new_in(parent)?;
        writeln!(temp, "{}", root.display())?;
        temp.persist(&cache).map_err(|err| err.error)?;
        Ok(true)
    };
    write()
}

fn cache_contents(path: &Path) -> io::Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(contents) => Ok(Some(contents)),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

fn cached_root(contents: &[u8]) -> Option<PathBuf> {
    let text = std::str::from_utf8(contents).ok()?;
    let cached = manifest::expand_path(text.trim());
    is_root(&cached).then(|| resolve(&cached))
}

pub fn read_root_cache() -> Option<PathBuf> {
    cached_root(&cache_contents(&root_cache_path()).ok()??)
}

/// Validate `root` as a registry without recording it (`set-root --dry-run`).
pub fn validate_repo_root(root: &Path) -> Result<PathBuf> {
    let resolved = resolve(&manifest::expand_path(&root.to_string_lossy()));
    if !is_root(&resolved) {
        return Err(Error::Manifest(format!(
            "{} does not contain {MANIFEST_FILE}",
            root.display()
        )));
    }
    Ok(resolved)
}

/// Validate and record `root` as the active registry.
pub fn cache_repo_root(root: &Path) -> Result<PathBuf> {
    let resolved = resolve(&manifest::expand_path(&root.to_string_lossy()));
    if !is_root(&resolved) {
        return Err(Error::Manifest(format!(
            "{} does not contain {MANIFEST_FILE}",
            root.display()
        )));
    }
    try_write_root_cache(&resolved, None).map_err(|err| Error::io(root_cache_path(), err))?;
    Ok(resolved)
}

pub fn find_cwd_root(start: Option<&Path>) -> Result<PathBuf> {
    let start = match start {
        Some(path) => path.to_path_buf(),
        None => std::env::current_dir().map_err(|e| Error::io(".", e))?,
    };
    let start = resolve(&start);
    for dir in start.ancestors() {
        if is_root(dir) {
            return Ok(dir.to_path_buf());
        }
    }
    Err(Error::Manifest(format!(
        "{MANIFEST_FILE} not found. Run this command from a registry, pass a \
         registry path, or run 'my-skills init-registry' first."
    )))
}

/// Resolve the registry root. `write_cache` selects the real-write policy:
/// record a first cwd discovery and refuse a cwd override of the active root.
pub fn resolve_root(start: Option<&Path>, write_cache: bool) -> Result<RootResolution> {
    resolve_root_inner(start, write_cache, write_cache)
}

fn resolve_root_inner(
    start: Option<&Path>,
    write_cache: bool,
    warn_mismatch: bool,
) -> Result<RootResolution> {
    let mut resolution = discover_root(start)?;
    if let Some(active) = different_active_root(&resolution) {
        if write_cache {
            return Err(Error::Manifest(write_refusal_message(
                &resolution.root,
                active,
            )));
        }
        if warn_mismatch {
            eprintln!(
                "note: using ./{MANIFEST_FILE} for this command; active registry is {}. \
                 Run 'my-skills set-root' here to switch.",
                paths::display_path(active)
            );
        }
    } else if write_cache && resolution.remember() {
        resolution.cached = Some(resolution.root.clone());
    }
    Ok(resolution)
}

fn different_active_root(resolution: &RootResolution) -> Option<&Path> {
    if resolution.source != RootSource::Cwd {
        return None;
    }
    resolution
        .cached
        .as_deref()
        .filter(|active| *active != resolve(&resolution.root))
}

/// Fail fast on a `--root` that is not a registry. `cli::run` calls it before
/// dispatch so every command reports the same usage error.
pub fn check_explicit_root() -> Result<()> {
    match explicit_root() {
        Some(path) if !is_root(&resolve(&manifest::expand_os_path(path))) => {
            Err(Error::Manifest(format!(
                "--root {} does not contain {MANIFEST_FILE}",
                paths::display_path(path)
            )))
        }
        _ => Ok(()),
    }
}

/// True when `error` is the "no registry configured anywhere" error from
/// [`resolve_root`], as opposed to an invalid `--root`, `MY_SKILLS_ROOT`, or
/// broken manifest.
pub fn is_unconfigured(error: &Error) -> bool {
    matches!(error, Error::Unconfigured)
}

fn discover_root(start: Option<&Path>) -> Result<RootResolution> {
    if let Some(path) = explicit_root() {
        let root = resolve(&manifest::expand_os_path(path));
        if !is_root(&root) {
            return Err(Error::Manifest(format!(
                "--root {} does not contain {MANIFEST_FILE}",
                paths::display_path(path)
            )));
        }
        return Ok(RootResolution {
            root,
            source: RootSource::Flag,
            cached: read_root_cache(),
            cache: CacheSnapshot::default(),
        });
    }

    if let Some(env) = std::env::var_os("MY_SKILLS_ROOT").filter(|v| !v.is_empty()) {
        let env = env.to_string_lossy().into_owned();
        let root = resolve(&manifest::expand_path(&env));
        if is_root(&root) {
            return Ok(RootResolution {
                root,
                source: RootSource::Env,
                cached: read_root_cache(),
                cache: CacheSnapshot::default(),
            });
        }
        return Err(Error::Manifest(format!(
            "MY_SKILLS_ROOT={env} does not contain {MANIFEST_FILE}"
        )));
    }

    let cache = CacheSnapshot::observe();
    let cached = cache.active();
    if let Ok(cwd_root) = find_cwd_root(start) {
        return Ok(RootResolution {
            root: cwd_root,
            source: RootSource::Cwd,
            cached,
            cache,
        });
    }

    if let Some(active) = cached {
        return Ok(RootResolution {
            root: active.clone(),
            source: RootSource::Cache,
            cached: Some(active),
            cache,
        });
    }

    Err(Error::Unconfigured)
}

/// Resolve the registry root and load its manifest. Pass `write_cache = false`
/// for read-only and preview paths to keep the stderr note without writing
/// to the config home.
pub fn load_manifest_resolved(
    cli: &CliOverrides,
    write_cache: bool,
) -> Result<(Manifest, RootResolution)> {
    let resolution = resolve_root_inner(None, write_cache, true)?;
    let manifest = manifest::load_manifest(&resolution.root, cli)?;
    Ok((manifest, resolution))
}

/// Load a manifest for inspection without writing the active-root cache.
pub fn load_manifest_from_cwd(cli: &CliOverrides) -> Result<Manifest> {
    load_manifest_resolved(cli, false).map(|(manifest, _)| manifest)
}

/// `None` or `"all"` means every enabled target; otherwise one known host.
pub fn resolve_hosts(manifest: &Manifest, host: Option<&str>) -> Result<Vec<String>> {
    match host {
        Some(name) if name != "all" => {
            if manifest.targets.contains_key(name) {
                Ok(vec![name.to_string()])
            } else {
                Err(Error::Manifest(format!("unknown host: {name}")))
            }
        }
        _ => Ok(manifest
            .targets
            .values()
            .filter(|t| t.enabled)
            .map(|t| t.name.clone())
            .collect()),
    }
}

/// v0.4.1 `select_requested`.
pub fn select_requested<'m>(
    manifest: &'m Manifest,
    skill: Option<&str>,
    all: bool,
    host: Option<&str>,
) -> Result<(Vec<&'m Skill>, Vec<String>)> {
    let explicit: Vec<String> = skill.map(|s| vec![s.to_string()]).unwrap_or_default();
    let skills = manifest::selected_skills(manifest, &explicit, all)?;
    let hosts = resolve_hosts(manifest, host)?;
    Ok((skills, hosts))
}

/// v0.4.1 `confirm_multi_host_write`: writing to more than one host needs
/// `--yes`. Prints the v0.4.1 error and returns false when refused.
pub fn confirm_multi_host_write(hosts: &[String], yes: bool) -> bool {
    if hosts.len() <= 1 || yes {
        return true;
    }
    eprintln!(
        "error: this command writes to multiple hosts ({}). \
         Re-run with --yes after reviewing a dry-run plan.",
        hosts.join(", ")
    );
    false
}
