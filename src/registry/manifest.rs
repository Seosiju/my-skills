//! `my-skills.toml` loading. Stream B. Spec §8.1, §8.2, decision D1.
//!
//! Mirrors v0.4.1 `config.py` with one deliberate change: the default
//! `install_mode` is `link` (D1), and the value is actually honored.
//!
//! Precedence, highest first: CLI overrides > `my-skills.local.toml` >
//! `my-skills.toml` > built-in default. Tables merge key by key, as in v0.4.1.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};
use std::{env, fs};

use serde::{Deserialize, Serialize};
use toml::{Table, Value};

use crate::{Error, Result, drive::hosts, platform::paths};

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
    /// Optional per-target install mode override (`[targets.<host>] install_mode`).
    pub install_mode: Option<InstallMode>,
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
    /// Explicit CLI `--mode` override, which takes precedence over target and default modes.
    pub cli_install_mode: Option<InstallMode>,
    pub audit: AuditSettings,
    /// Built-in hosts and declared targets with paths, sorted by name.
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

    /// Effective install mode for a new connection on `host`:
    /// CLI `--mode` > `[targets.<host>] install_mode` > `[defaults] install_mode`.
    pub fn effective_install_mode(&self, host: &str) -> InstallMode {
        self.cli_install_mode
            .or_else(|| {
                self.targets
                    .get(host)
                    .and_then(|target| target.install_mode)
            })
            .unwrap_or(self.defaults.install_mode)
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
/// invalid TOML (with file path), enabled target without a path or built-in
/// default. Disabled targets without a path or built-in default are omitted.
pub fn load_manifest(root: &Path, cli: &CliOverrides) -> Result<Manifest> {
    let main_path = root.join(MANIFEST_FILE);
    let main = read_table(&main_path)?
        .ok_or_else(|| Error::Manifest(format!("manifest not found: {}", main_path.display())))?;
    let local_path = root.join(LOCAL_MANIFEST_FILE);
    let local = read_table(&local_path)?.unwrap_or_default();

    let root_values = Layers {
        main: Some(&main),
        local: Some(&local),
    };
    let values = root_values.table("defaults");
    let mut defaults = Defaults::default();
    let file_mode = values
        .local
        .and_then(|table| table.get("install_mode"))
        .map(|value| (value, &local_path))
        .or_else(|| {
            values
                .main
                .and_then(|table| table.get("install_mode"))
                .map(|value| (value, &main_path))
        });
    if let Some(mode) = cli.install_mode {
        defaults.install_mode = mode;
    } else if let Some((value, path)) = file_mode {
        defaults.install_mode = match value.as_str() {
            Some("link") => InstallMode::Link,
            Some("copy") => InstallMode::Copy,
            _ => {
                return Err(Error::Manifest(format!(
                    "invalid install_mode in {}: {value}; expected \"link\" or \"copy\"",
                    path.display(),
                )));
            }
        };
    }
    if let Some(collision) = values.string("collision") {
        defaults.collision = collision.into();
    }
    defaults.verify_after_install = values.boolean("verify_after_install", true);

    let values = root_values.table("audit");
    let audit = AuditSettings {
        enabled: values.boolean("enabled", true),
        profile: values.string("profile").unwrap_or("default").into(),
        threshold: values
            .string("threshold")
            .filter(|value| !value.is_empty())
            .map(str::to_owned),
        disabled_rules: values.strings("disabled_rules"),
    };

    let skills_root = cli
        .skills_root
        .as_deref()
        .filter(|value| !value.is_empty())
        .or_else(|| {
            local
                .get("skills_root")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
        })
        .or_else(|| main.get("skills_root").and_then(Value::as_str))
        .unwrap_or("skills");

    Ok(Manifest {
        schema_version: schema_version(&main),
        root: root.to_owned(),
        skills_root: skills_root.into(),
        defaults,
        cli_install_mode: cli.install_mode,
        audit,
        targets: resolve_targets(root_values.table("targets"), &main_path, &local_path, cli)?,
        skills: resolve_skills(root_values.table("skills")),
    })
}

fn read_table(path: &Path) -> Result<Option<Table>> {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => {}
        Ok(_) => return Ok(None),
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(Error::io(path, error)),
    }
    let contents = fs::read_to_string(path).map_err(|error| match error.kind() {
        ErrorKind::InvalidData => {
            Error::Manifest(format!("invalid TOML in {}: {error}", path.display()))
        }
        _ => Error::io(path, error),
    })?;
    contents
        .parse()
        .map(Some)
        .map_err(|error| Error::Manifest(format!("invalid TOML in {}: {error}", path.display())))
}

struct Layers<'a> {
    main: Option<&'a Table>,
    local: Option<&'a Table>,
}

impl<'a> Layers<'a> {
    fn table(&self, key: &str) -> Self {
        let subtable = |table: &'a Table| table.get(key).and_then(Value::as_table);
        Self {
            main: self.main.and_then(subtable),
            local: self.local.and_then(subtable),
        }
    }

    fn keys(&self) -> impl Iterator<Item = &'a str> {
        self.main
            .into_iter()
            .chain(self.local)
            .flat_map(Table::keys)
            .map(String::as_str)
    }

    fn get(&self, key: &str) -> Option<&Value> {
        let lookup = |table: &'a Table| table.get(key);
        self.local
            .and_then(lookup)
            .or_else(|| self.main.and_then(lookup))
    }

    fn string(&self, key: &str) -> Option<&str> {
        self.get(key).and_then(Value::as_str)
    }

    fn boolean(&self, key: &str, default: bool) -> bool {
        self.get(key).and_then(Value::as_bool).unwrap_or(default)
    }

    fn strings(&self, key: &str) -> Vec<String> {
        self.get(key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect()
    }
}

fn schema_version(main: &Table) -> i64 {
    match main.get("schema_version") {
        Some(Value::Integer(version)) => *version,
        Some(Value::Boolean(version)) => i64::from(*version),
        Some(Value::String(version)) => version.trim().parse().unwrap_or(1),
        _ => 1,
    }
}

fn resolve_targets(
    target_values: Layers<'_>,
    main_path: &Path,
    local_path: &Path,
    cli: &CliOverrides,
) -> Result<BTreeMap<String, Target>> {
    let names: BTreeSet<&str> = hosts::ALL
        .iter()
        .map(|host| host.name)
        .chain(target_values.keys())
        .chain(cli.target_paths.keys().map(String::as_str))
        .collect();

    let mut targets = BTreeMap::new();
    for name in names {
        let values = target_values.table(name);
        let host = hosts::get_host(name);
        let enabled = values.boolean("enabled", host.is_none_or(|host| host.default_enabled));
        let default_path = host.map(|host| Path::new(host.default_user_path));
        let path = match cli.target_paths.get(name) {
            Some(path) if !path.as_os_str().is_empty() => Some(path.as_path()),
            // An explicitly empty CLI path falls back to the built-in default.
            Some(_) => default_path,
            None => values
                .string("path")
                .filter(|path| !path.is_empty())
                .map(Path::new)
                .or(default_path),
        };
        if path.is_none() && !enabled {
            continue;
        }

        let target_mode = values
            .local
            .and_then(|table| table.get("install_mode"))
            .map(|value| (value, local_path))
            .or_else(|| {
                values
                    .main
                    .and_then(|table| table.get("install_mode"))
                    .map(|value| (value, main_path))
            });
        let install_mode = match target_mode {
            None => None,
            Some((value, path)) => Some(match value.as_str() {
                Some("link") => InstallMode::Link,
                Some("copy") => InstallMode::Copy,
                _ => {
                    return Err(Error::Manifest(format!(
                        "invalid install_mode for target '{name}' in {}: {value}; expected \"link\" or \"copy\"",
                        path.display(),
                    )));
                }
            }),
        };
        let path = path.ok_or_else(|| {
            Error::Manifest(format!(
                "target '{name}' has no path and no built-in default"
            ))
        })?;
        let target = Target {
            name: name.into(),
            enabled,
            scope: values.string("scope").unwrap_or("user").into(),
            path: expand_os_path(path),
            install_mode,
        };
        targets.insert(name.into(), target);
    }
    Ok(targets)
}

fn resolve_skills(skill_values: Layers<'_>) -> BTreeMap<String, Skill> {
    let names: BTreeSet<_> = skill_values.keys().collect();
    names
        .into_iter()
        .map(|name| {
            let values = skill_values.table(name);
            let skill = Skill {
                name: name.into(),
                enabled: values.boolean("enabled", true),
                hosts: values.strings("hosts"),
                source_type: values.string("source_type").unwrap_or_default().into(),
                source_revision: values.string("source_revision").unwrap_or_default().into(),
                group: values.string("group").map(str::to_owned),
            };
            (name.into(), skill)
        })
        .collect()
}

/// Expand `~` and `$VAR` / `${VAR}` (and `%VAR%` on Windows) like Python
/// `os.path.expandvars` + `expanduser`. Relative results are joined to the
/// current directory. Links are not resolved. Uses `paths::home_dir()`.
pub fn expand_path(value: &str) -> PathBuf {
    absolute_path(Path::new(&expand_vars(value)))
}

fn expand_os_path(path: &Path) -> PathBuf {
    path.to_str()
        .map_or_else(|| absolute_path(path), expand_path)
}

fn absolute_path(path: &Path) -> PathBuf {
    let mut components = path.components();
    let path = if matches!(components.next(), Some(Component::Normal(name)) if name == "~") {
        // Push component by component so the result uses the OS separator.
        // Gluing "~/a/b" onto a Windows home as text leaves '/' inside the
        // path, which a verbatim (\\?\) path treats as part of a name.
        let mut home = paths::home_dir();
        for component in components {
            home.push(component);
        }
        home
    } else {
        path.to_owned()
    };
    if path.is_absolute() {
        return path;
    }
    let joined = env::current_dir()
        .expect("could not resolve the current directory")
        .join(&path);
    #[cfg(windows)]
    if !joined.is_absolute() {
        // A drive-relative path may refer to the current directory of another drive.
        return std::path::absolute(path).expect("could not resolve the drive-relative path");
    }
    joined
}

fn expand_vars(mut remaining: &str) -> OsString {
    let mut expanded = OsString::new();
    while let Some(start) =
        remaining.find(|ch| ch == '$' || cfg!(windows) && matches!(ch, '%' | '\''))
    {
        expanded.push(&remaining[..start]);
        let candidate = &remaining[start..];
        let marker = candidate.as_bytes()[0];
        let tail = &candidate[1..];
        if cfg!(windows)
            && matches!(marker, b'$' | b'%')
            && tail.as_bytes().first() == Some(&marker)
        {
            expanded.push(&candidate[..1]);
            remaining = &tail[1..];
            continue;
        }

        let (length, name) = match marker {
            b'\'' => (tail.find('\'').map_or(candidate.len(), |end| end + 2), None),
            b'%' => delimited_var(candidate, 1, '%'),
            b'$' if tail.starts_with('{') => delimited_var(candidate, 2, '}'),
            _ => {
                let length = tail
                    .bytes()
                    .take_while(|byte| {
                        byte.is_ascii_alphanumeric()
                            || *byte == b'_'
                            || cfg!(windows) && *byte == b'-'
                    })
                    .count();
                (1 + length, Some(&tail[..length]))
            }
        };
        if let Some(value) = name
            .filter(|name| !name.is_empty() && !name.contains(['=', '\0']))
            .and_then(env::var_os)
        {
            expanded.push(value);
        } else {
            expanded.push(&candidate[..length]);
        }
        remaining = &candidate[length..];
    }
    expanded.push(remaining);
    expanded
}

fn delimited_var(candidate: &str, prefix_len: usize, close: char) -> (usize, Option<&str>) {
    match candidate[prefix_len..].find(close) {
        Some(end) => (
            prefix_len + end + 1,
            Some(&candidate[prefix_len..prefix_len + end]),
        ),
        None => (candidate.len(), None),
    }
}

/// v0.4.1 `selected_skills`: explicit names (each must exist), or all with
/// `all`, or only enabled skills.
pub fn selected_skills<'m>(
    manifest: &'m Manifest,
    explicit: &[String],
    all: bool,
) -> Result<Vec<&'m Skill>> {
    if explicit.is_empty() {
        return Ok(manifest
            .skills
            .values()
            .filter(|skill| all || skill.enabled)
            .collect());
    }

    let mut names: Vec<_> = explicit.iter().map(String::as_str).collect();
    names.sort_unstable();
    let missing: Vec<_> = names
        .iter()
        .copied()
        .filter(|name| !manifest.skills.contains_key(*name))
        .collect();
    if !missing.is_empty() {
        return Err(Error::Manifest(format!(
            "unknown skill(s): {}",
            missing.join(", ")
        )));
    }
    Ok(names
        .into_iter()
        .map(|name| &manifest.skills[name])
        .collect())
}

#[cfg(test)]
mod expand_path_separator_tests {
    use super::*;

    #[test]
    fn tilde_paths_use_only_the_os_separator() {
        let expanded = expand_path("~/.gemini/config/skills");
        assert!(expanded.ends_with(Path::new(".gemini").join("config").join("skills")));
        #[cfg(windows)]
        assert!(
            !expanded.to_string_lossy().contains('/'),
            "mixed separators break verbatim paths: {}",
            expanded.display()
        );
    }
}
