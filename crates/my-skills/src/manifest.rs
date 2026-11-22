//! `my-skills.toml` loading. Stream B. Spec §8.1, §8.2, decision D1.
//!
//! Mirrors v0.4.1 `config.py` with one deliberate change: the default
//! `install_mode` is `link` (D1), and the value is actually honored.
//!
//! Precedence, highest first: CLI overrides > `my-skills.local.toml` >
//! `my-skills.toml` > built-in default. Tables merge key by key, as in v0.4.1.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::Result;

pub const MANIFEST_FILE: &str = "my-skills.toml";
pub const LOCAL_MANIFEST_FILE: &str = "my-skills.local.toml";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InstallMode {
    #[default]
    Link,
    Copy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Defaults {
    /// Default `Link` when absent. An unknown value is a manifest error.
    pub install_mode: InstallMode,
    pub collision: String,
    pub verify_after_install: bool,
}

impl Default for Defaults {
    fn default() -> Self {
        Defaults {
            install_mode: InstallMode::Link,
            collision: "error".into(),
            verify_after_install: true,
        }
    }
}

/// `[audit]` table, passed to `audit::policy_from_settings`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditSettings {
    pub enabled: bool,
    pub profile: String,
    pub threshold: Option<String>,
    pub disabled_rules: Vec<String>,
}

impl Default for AuditSettings {
    fn default() -> Self {
        AuditSettings {
            enabled: true,
            profile: "default".into(),
            threshold: None,
            disabled_rules: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub name: String,
    /// Default from `hosts::get_host(name).default_enabled`, else true.
    pub enabled: bool,
    pub scope: String,
    /// `~` and environment variables expanded, absolute, links not resolved.
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skill {
    pub name: String,
    pub enabled: bool,
    /// Empty means every host.
    pub hosts: Vec<String>,
    pub source_type: String,
    pub source_revision: String,
    /// Display-only grouping read by registries; the CLI ignores it.
    pub group: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub schema_version: i64,
    pub root: PathBuf,
    pub skills_root: String,
    pub defaults: Defaults,
    pub audit: AuditSettings,
    /// Every built-in host plus every declared target, sorted by name.
    pub targets: BTreeMap<String, Target>,
    pub skills: BTreeMap<String, Skill>,
}

impl Manifest {
    pub fn skills_dir(&self) -> PathBuf {
        self.root.join(&self.skills_root)
    }

    /// Canonical directory of one skill.
    pub fn skill_dir(&self, name: &str) -> PathBuf {
        self.skills_dir().join(name)
    }
}

/// Overrides from the command line (for example `--mode`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CliOverrides {
    pub install_mode: Option<InstallMode>,
    pub skills_root: Option<String>,
    /// target name -> path
    pub target_paths: BTreeMap<String, PathBuf>,
}

/// Load `root/my-skills.toml` plus the optional local file.
///
/// Errors (as `Error::Manifest`) mirror v0.4.1 messages: missing manifest,
/// invalid TOML (with file path), target without a path and no built-in
/// default.
pub fn load_manifest(_root: &Path, _cli: &CliOverrides) -> Result<Manifest> {
    todo!("stream B: spec §8.1, §8.2")
}

/// Expand `~` and `$VAR` / `${VAR}` (and `%VAR%` on Windows) like Python
/// `os.path.expandvars` + `expanduser`. Relative results are joined to the
/// current directory. Links are not resolved. Uses `paths::home_dir()`.
pub fn expand_path(_value: &str) -> PathBuf {
    todo!("stream B")
}

/// v0.4.1 `selected_skills`: explicit names (each must exist), or all with
/// `all`, or only enabled skills.
pub fn selected_skills<'m>(
    _manifest: &'m Manifest,
    _explicit: &[String],
    _all: bool,
) -> Result<Vec<&'m Skill>> {
    todo!("stream B")
}
