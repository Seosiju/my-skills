//! Machine-local install state. Stream B. Spec §8.3, decision D6.
//!
//! Reads every `state.json` written by v0.4.1 (schema 1). New fields are
//! optional; when they are missing, callers infer them from the filesystem.
//! Field order matches v0.4.1 `asdict` so diffs between versions stay small.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write as _};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::drive::link::LinkKind;
use crate::registry::manifest::InstallMode;
use crate::{Error, Result};

pub const SCHEMA_VERSION: i64 = 1;

const REQUIRED_RECORD_FIELDS: &[&str] = &[
    "destination",
    "host",
    "installed_at",
    "installed_hash",
    "mode",
    "skill",
    "source",
    "source_hash",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallRecord {
    pub skill: String,
    pub host: String,
    pub mode: InstallMode,
    pub source: String,
    pub destination: String,
    pub source_hash: String,
    pub installed_hash: String,
    pub installed_at: String,
    #[serde(default = "default_source_type")]
    pub source_type: String,
    #[serde(default)]
    pub source_url: String,
    #[serde(default)]
    pub source_revision: String,
    #[serde(default)]
    pub last_audit_at: String,
    #[serde(default)]
    pub last_audit_result_hash: String,
    #[serde(default)]
    pub audit_profile: String,
    #[serde(default)]
    pub audit_threshold: String,
    /// New in v1. Absent for v0.4.1 records.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link_kind: Option<LinkKind>,
    /// New in v1. Why a link install fell back to copy (spec §4 I4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_reason: Option<String>,
}

fn default_source_type() -> String {
    "canonical".into()
}

impl InstallRecord {
    pub fn key(&self) -> (String, String) {
        (self.skill.clone(), self.host.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct State {
    pub path: PathBuf,
    pub installs: BTreeMap<(String, String), InstallRecord>,
}

#[derive(Serialize)]
struct StateFile<'a> {
    schema_version: i64,
    installs: Vec<&'a InstallRecord>,
}

/// `paths::state_home()/my-skills/state.json`.
pub fn default_state_path() -> PathBuf {
    crate::platform::paths::state_home()
        .join("my-skills")
        .join("state.json")
}

impl State {
    /// Missing file -> empty state. Errors (as `Error::State`) mirror v0.4.1:
    /// newer schema, non-object record, missing required fields. Unknown
    /// record fields are ignored.
    pub fn load(path: Option<&Path>) -> Result<State> {
        let path = path.map_or_else(default_state_path, Path::to_owned);
        let mut state = Self {
            path,
            installs: BTreeMap::new(),
        };
        let path = &state.path;
        match fs::metadata(path) {
            Ok(metadata) if metadata.is_file() => {}
            Ok(_) => return Ok(state),
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(state),
            Err(error) => return Err(Error::io(path, error)),
        }

        let contents = fs::read(path).map_err(|error| Error::io(path, error))?;
        let data: Value = serde_json::from_slice(&contents)
            .map_err(|error| state_error(path, format_args!("contains invalid JSON: {error}")))?;
        let data = data
            .as_object()
            .ok_or_else(|| state_error(path, "must contain a JSON object"))?;
        if let Some(version) = data.get("schema_version") {
            let newer = match version {
                Value::Number(number) => number
                    .as_f64()
                    .map(|value| value.trunc() > SCHEMA_VERSION as f64),
                Value::String(value) => value
                    .trim()
                    .parse::<i64>()
                    .ok()
                    .map(|value| value > SCHEMA_VERSION),
                Value::Bool(value) => Some(i64::from(*value) > SCHEMA_VERSION),
                _ => None,
            }
            .ok_or_else(|| {
                state_error(
                    path,
                    format_args!("has an invalid schema_version: {version}"),
                )
            })?;
            if newer {
                return Err(state_error(path, "was written by a newer my-skills"));
            }
        }

        let Some(installs) = data.get("installs") else {
            return Ok(state);
        };
        let installs = installs
            .as_array()
            .ok_or_else(|| state_error(path, "must contain an installs array"))?;
        for (index, raw) in installs.iter().enumerate() {
            let fields = raw.as_object().ok_or_else(|| {
                state_error(
                    path,
                    format_args!("contains a malformed install record at index {index}"),
                )
            })?;
            let missing: Vec<_> = REQUIRED_RECORD_FIELDS
                .iter()
                .copied()
                .filter(|field| !fields.contains_key(*field))
                .collect();
            if !missing.is_empty() {
                return Err(state_error(
                    path,
                    format_args!(
                        "install record at index {index} is missing required field(s): {}",
                        missing.join(", ")
                    ),
                ));
            }
            let record: InstallRecord = InstallRecord::deserialize(raw).map_err(|error| {
                state_error(
                    path,
                    format_args!("contains a malformed install record at index {index} ({error})"),
                )
            })?;
            state.installs.insert(record.key(), record);
        }
        Ok(state)
    }

    /// Atomic write: temp file in the same directory, then rename.
    /// Records sorted by (skill, host); JSON indented by 2 with a trailing
    /// newline, as v0.4.1 writes it.
    pub fn save(&self) -> Result<()> {
        let payload = StateFile {
            schema_version: SCHEMA_VERSION,
            installs: self.records().collect(),
        };
        let json = serde_json::to_string_pretty(&payload).map_err(|error| {
            Error::State(format!(
                "could not serialize state file {}: {error}",
                self.path.display()
            ))
        })?;
        let contents = python_json_bytes(&json);

        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| Error::io(parent, error))?;
        }
        let mut temp_path = self.path.as_os_str().to_owned();
        temp_path.push(".tmp");
        let temp_path = PathBuf::from(temp_path);

        // Reserve the sibling before writing so another writer's staging file is protected.
        let mut temp = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)
            .map_err(|error| Error::io(&temp_path, error))?;
        let written = temp
            .write_all(contents.as_bytes())
            .and_then(|()| temp.sync_all());
        drop(temp);
        let result = written
            .map_err(|error| Error::io(&temp_path, error))
            .and_then(|()| {
                fs::rename(&temp_path, &self.path).map_err(|error| Error::io(&self.path, error))
            });
        if result.is_err() {
            let _ = fs::remove_file(&temp_path);
        }
        result
    }

    pub fn get(&self, skill: &str, host: &str) -> Option<&InstallRecord> {
        self.installs.get(&(skill.to_string(), host.to_string()))
    }

    pub fn put(&mut self, record: InstallRecord) {
        self.installs.insert(record.key(), record);
    }

    pub fn remove(&mut self, skill: &str, host: &str) {
        self.installs.remove(&(skill.to_string(), host.to_string()));
    }

    pub fn records(&self) -> impl Iterator<Item = &InstallRecord> {
        self.installs.values()
    }
}

fn state_error(path: &Path, detail: impl std::fmt::Display) -> Error {
    let path = path.display();
    Error::State(format!(
        "state file {path} {detail}; upgrade the CLI, or move {path} aside and re-run install"
    ))
}

fn python_json_bytes(json: &str) -> String {
    // v0.4.1 uses json.dumps with ensure_ascii=True, including surrogate pairs.
    let mut escaped = String::with_capacity(json.len() + 1);
    for ch in json.chars() {
        if ch < '\u{7f}' {
            escaped.push(ch);
        } else {
            for unit in ch.encode_utf16(&mut [0; 2]) {
                write!(escaped, "\\u{unit:04x}").expect("writing to a String cannot fail");
            }
        }
    }
    escaped.push('\n');
    escaped
}
