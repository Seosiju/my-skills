//! Shared command runtime: registry root resolution, host and skill
//! selection, write confirmation. Integrator. Mirrors v0.4.1 `cli_runtime.py`
//! and `write_confirmation.py`.
//!
//! Root resolution order (highest first):
//! 1. `$MY_SKILLS_ROOT` (an invalid value is an error),
//! 2. `my-skills.toml` in the current directory or a parent; cached only when
//!    no valid active root exists yet,
//! 3. the cached active root (`$XDG_CONFIG_HOME/my-skills/root`).

use std::fs;
use std::path::{Path, PathBuf};

use crate::manifest::{self, CliOverrides, MANIFEST_FILE, Manifest, Skill};
use crate::{Error, Result, paths};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootSource {
    Env,
    Cwd,
    Cache,
}

impl RootSource {
    pub fn label(self) -> &'static str {
        match self {
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

fn write_root_cache(root: &Path) {
    let cache = root_cache_path();
    if let Some(parent) = cache.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(&cache, format!("{}\n", root.display()));
}

pub fn read_root_cache() -> Option<PathBuf> {
    let text = fs::read_to_string(root_cache_path()).ok()?;
    let cached = manifest::expand_path(text.trim());
    is_root(&cached).then(|| resolve(&cached))
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
    write_root_cache(&resolved);
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

/// Resolve the registry root. With `write_cache`, a first-ever discovery is
/// cached, and a cwd registry that differs from the active one prints a note.
pub fn resolve_root(start: Option<&Path>, write_cache: bool) -> Result<RootResolution> {
    if let Some(env) = std::env::var_os("MY_SKILLS_ROOT").filter(|v| !v.is_empty()) {
        let env = env.to_string_lossy().into_owned();
        let root = resolve(&manifest::expand_path(&env));
        if is_root(&root) {
            return Ok(RootResolution {
                root,
                source: RootSource::Env,
                cached: read_root_cache(),
            });
        }
        return Err(Error::Manifest(format!(
            "MY_SKILLS_ROOT={env} does not contain {MANIFEST_FILE}"
        )));
    }

    let mut cached = read_root_cache();
    if let Ok(cwd_root) = find_cwd_root(start) {
        if write_cache {
            match &cached {
                None => {
                    write_root_cache(&cwd_root);
                    cached = Some(cwd_root.clone());
                }
                Some(active) if *active != resolve(&cwd_root) => {
                    eprintln!(
                        "note: using ./{MANIFEST_FILE} for this command; active registry is {}. \
                         Run 'my-skills set-root' here to switch.",
                        active.display()
                    );
                }
                Some(_) => {}
            }
        }
        return Ok(RootResolution {
            root: cwd_root,
            source: RootSource::Cwd,
            cached,
        });
    }

    if let Some(active) = cached {
        return Ok(RootResolution {
            root: active.clone(),
            source: RootSource::Cache,
            cached: Some(active),
        });
    }

    Err(Error::Manifest(format!(
        "{MANIFEST_FILE} not found. Run 'my-skills init-registry' to create a \
         registry, run 'my-skills set-root <path>' to select one, or set \
         MY_SKILLS_ROOT to its path."
    )))
}

/// v0.4.1 `load_manifest_from_cwd`.
pub fn load_manifest_from_cwd(cli: &CliOverrides) -> Result<Manifest> {
    let resolution = resolve_root(None, true)?;
    manifest::load_manifest(&resolution.root, cli)
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
