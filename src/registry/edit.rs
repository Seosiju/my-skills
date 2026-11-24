//! Comment-preserving manifest edits (v0.4.1 manifest_edit.py). Stream H.

use std::fmt::Write as _;
use std::fs;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

use tempfile::NamedTempFile;

use crate::check::frontmatter::python_whitespace;
use crate::check::validate::validate_name;
use crate::drive::link::{EntryKind, entry_kind};
use crate::{Error, Result};

pub fn set_skill_enabled(path: &Path, name: &str, enabled: bool) -> Result<()> {
    prepare_enabled(path, &[name], enabled)?.commit()
}

pub fn register_skill(path: &Path, name: &str, enabled: bool, hosts: &[String]) -> Result<()> {
    prepare_registration(path, name, enabled, hosts)?.commit()
}

/// Prepared edits retain the old bytes so a failed share can restore the
/// manifest as well as the canonical skill.
pub(crate) struct ManifestEdit {
    path: PathBuf,
    before: String,
    after: String,
    _lock: Option<EditLock>,
}

// Hold the lock through the canonical swap and state save so two registry
// writers cannot publish conflicting edits or roll back each other's work.
pub(crate) struct EditLock(PathBuf);

impl EditLock {
    fn path(manifest: &Path) -> PathBuf {
        manifest.with_extension("toml.edit-lock")
    }

    pub(crate) fn check_available(manifest: &Path) -> Result<()> {
        let path = Self::path(manifest);
        match fs::symlink_metadata(&path) {
            Ok(_) => Err(Error::io(path, io::ErrorKind::AlreadyExists.into())),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(Error::io(path, err)),
        }
    }

    pub(crate) fn acquire(manifest: &Path) -> Result<Self> {
        let path = Self::path(manifest);
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|err| Error::io(&path, err))?;
        Ok(Self(path))
    }
}

impl Drop for EditLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

impl ManifestEdit {
    pub(crate) fn commit(&self) -> Result<()> {
        self.replace(&self.before, &self.after)
    }

    pub(crate) fn rollback(&self) -> Result<()> {
        self.replace(&self.after, &self.before)
    }

    fn replace(&self, before: &str, after: &str) -> Result<()> {
        if before == after {
            return Ok(());
        }
        if entry_kind(&self.path)? != EntryKind::File {
            return Err(Error::Refused(format!(
                "manifest is not a regular file: {}",
                self.path.display()
            )));
        }
        let parent = self.path.parent().filter(|p| !p.as_os_str().is_empty());
        let mut temp = NamedTempFile::new_in(parent.unwrap_or(Path::new(".")))
            .map_err(|err| Error::io(&self.path, err))?;
        let io_error = |err| Error::io(&self.path, err);
        let permissions = fs::metadata(&self.path).map_err(io_error)?.permissions();
        temp.write_all(after.as_bytes()).map_err(io_error)?;
        temp.as_file()
            .set_permissions(permissions)
            .map_err(io_error)?;
        temp.as_file().sync_all().map_err(io_error)?;
        if read(&self.path)? != before {
            return Err(Error::Manifest(format!(
                "{} changed during the edit; re-run the command",
                self.path.display()
            )));
        }
        temp.persist(&self.path)
            .map_err(|err| io_error(err.error))?;
        Ok(())
    }
}

pub(crate) fn prepare_enabled(
    path: &Path,
    names: &[impl AsRef<str>],
    enabled: bool,
) -> Result<ManifestEdit> {
    prepare(path, |lines| {
        for name in names {
            let name = name.as_ref();
            let (start, end) = skill_section(lines, name)
                .ok_or_else(|| Error::Manifest(format!("unknown skill: {name}")))?;
            set_key(
                lines,
                start + 1,
                end,
                "enabled",
                format!("enabled = {enabled}"),
            );
        }
        Ok(true)
    })
}

/// Return the chosen enabled state. A local overlay can affect the displayed
/// state but isn't written back into the main manifest.
pub(crate) fn prepare_import(
    path: &Path,
    name: &str,
    enable: bool,
    current: bool,
    hosts: &[String],
    dry_run: bool,
) -> Result<(ManifestEdit, bool)> {
    let lock = if dry_run {
        // Report an edit already in progress without creating a lock or waiting.
        EditLock::check_available(path)?;
        None
    } else {
        Some(EditLock::acquire(path)?)
    };
    let mut enabled = enable;
    let edit = prepare_locked(path, lock, |lines| {
        if let Some((start, end)) = skill_section(lines, name) {
            if !enable {
                enabled = current;
                return Ok(false);
            }
            set_key(lines, start + 1, end, "enabled", "enabled = true".into());
        } else {
            register(lines, name, enable, hosts)?;
        }
        Ok(true)
    })?;
    Ok((edit, enabled))
}

pub(crate) fn prepare_registration(
    path: &Path,
    name: &str,
    enabled: bool,
    hosts: &[String],
) -> Result<ManifestEdit> {
    let lock = EditLock::acquire(path)?;
    prepare_registration_locked(path, name, enabled, hosts, lock)
}

pub(crate) fn prepare_registration_locked(
    path: &Path,
    name: &str,
    enabled: bool,
    hosts: &[String],
    lock: EditLock,
) -> Result<ManifestEdit> {
    prepare_locked(path, Some(lock), |lines| {
        register(lines, name, enabled, hosts)?;
        Ok(true)
    })
}

/// Register a brand-new skill as disabled. Unlike `register`, `group` and
/// `hosts` are written only when given, and an existing section is refused.
pub(crate) fn prepare_new_skill(
    path: &Path,
    name: &str,
    group: Option<&str>,
    hosts: &[String],
) -> Result<ManifestEdit> {
    // The caller holds the `EditLock` across the canonical write and any rollback.
    prepare_locked(path, None, |lines| {
        if let Some(error) = validate_name(name).first() {
            return Err(Error::Manifest(format!(
                "invalid skill name '{name}': {error}"
            )));
        }
        if skill_section(lines, name).is_some() {
            return Err(Error::Manifest(format!(
                "skill already exists: {name} is already registered"
            )));
        }
        if lines.last().is_some_and(|line| !line.is_empty()) {
            lines.push(String::new());
        }
        lines.push(format!("[skills.{name}]"));
        lines.push("enabled = false".into());
        if let Some(group) = group {
            lines.push(format!("group = {}", toml_string(group)));
        }
        if !hosts.is_empty() {
            let hosts = hosts
                .iter()
                .map(|host| toml_string(host))
                .collect::<Vec<_>>();
            lines.push(format!("hosts = [{}]", hosts.join(", ")));
        }
        Ok(true)
    })
}

fn register(lines: &mut Vec<String>, name: &str, enabled: bool, hosts: &[String]) -> Result<()> {
    if let Some(error) = validate_name(name).first() {
        return Err(Error::Manifest(format!(
            "invalid skill name '{name}': {error}"
        )));
    }
    let enabled = format!("enabled = {enabled}");
    let hosts = hosts
        .iter()
        .map(|host| toml_string(host))
        .collect::<Vec<_>>();
    let hosts = format!("hosts = [{}]", hosts.join(", "));
    if let Some((start, end)) = skill_section(lines, name) {
        set_key(lines, start + 1, end, "enabled", enabled);
        // v0.4.1 reuses the original end, even when adding both keys.
        set_key(lines, start + 1, end, "hosts", hosts);
    } else {
        if lines.last().is_some_and(|line| !line.is_empty()) {
            lines.push(String::new());
        }
        lines.extend([format!("[skills.{name}]"), enabled, hosts]);
    }
    Ok(())
}

fn prep_lines(text: &str) -> Vec<String> {
    text.replace("\r\n", "\n")
        .split_terminator(|c| {
            matches!(
                c,
                '\n' | '\r' | '\u{b}' | '\u{c}' | '\u{1c}'
                    ..='\u{1e}' | '\u{85}' | '\u{2028}' | '\u{2029}'
            )
        })
        .map(str::to_owned)
        .collect()
}

fn prepare(
    path: &Path,
    edit: impl FnOnce(&mut Vec<String>) -> Result<bool>,
) -> Result<ManifestEdit> {
    prepare_locked(path, Some(EditLock::acquire(path)?), edit)
}

fn prepare_locked(
    path: &Path,
    lock: Option<EditLock>,
    edit: impl FnOnce(&mut Vec<String>) -> Result<bool>,
) -> Result<ManifestEdit> {
    let before = read(path)?;
    let mut lines = prep_lines(&before);
    let after = if edit(&mut lines)? {
        let joined = lines.join("\n");
        let normalized = format!("{}\n", joined.trim_end_matches(python_whitespace));
        // A section syntax that the legacy line matcher cannot edit must
        // never leave the caller with an invalid registry configuration.
        toml::from_str::<toml::Table>(&normalized)
            .map_err(|err| Error::Manifest(format!("invalid edit to {}: {err}", path.display())))?;
        native_text(&normalized)
    } else {
        before.clone()
    };
    Ok(ManifestEdit {
        path: path.into(),
        before,
        after,
        _lock: lock,
    })
}

fn read(path: &Path) -> Result<String> {
    fs::read_to_string(path).map_err(|err| Error::io(path, err))
}

fn skill_section(lines: &[String], name: &str) -> Option<(usize, usize)> {
    let target = format!("skills.{name}");
    let mut headers = lines.iter().enumerate().filter_map(|(index, line)| {
        let inner = line
            .trim_matches(python_whitespace)
            .strip_prefix('[')?
            .strip_suffix(']')?;
        (!inner.is_empty() && !inner.contains(']')).then_some((index, inner))
    });
    let (start, _) = headers.find(|(_, header)| *header == target)?;
    Some((start, headers.next().map_or(lines.len(), |(end, _)| end)))
}

fn set_key(lines: &mut Vec<String>, start: usize, end: usize, key: &str, value: String) {
    let is_key = |line: &String| {
        line.trim_start_matches(python_whitespace)
            .strip_prefix(key)
            .is_some_and(|rest| rest.trim_start_matches(python_whitespace).starts_with('='))
    };
    if let Some(line) = lines[start..end].iter_mut().find(|line| is_key(line)) {
        *line = value;
    } else {
        lines.insert(end, value);
    }
}

fn toml_string(value: &str) -> String {
    // TOML uses Unicode escapes rather than JSON's short escapes for these
    // characters, and accepts Unicode scalars directly (including non-BMP).
    let json = serde_json::to_string(value).expect("a string is serializable");
    let mut escaped = String::new();
    for ch in json.chars() {
        match u32::from(ch) {
            0..=0x7e => escaped.push(ch),
            code @ 0x7f..=0xffff => write!(escaped, "\\u{code:04x}").unwrap(),
            code => write!(escaped, "\\U{code:08x}").unwrap(),
        }
    }
    escaped
}

pub(crate) fn native_text(text: &str) -> String {
    if cfg!(windows) {
        text.replace('\n', "\r\n")
    } else {
        text.to_owned()
    }
}
