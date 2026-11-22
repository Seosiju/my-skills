//! Machine-local install state. Stream B. Spec §8.3, decision D6.
//!
//! Reads every `state.json` written by v0.4.1 (schema 1). New fields are
//! optional; when they are missing, callers infer them from the filesystem.
//! Field order matches v0.4.1 `asdict` so diffs between versions stay small.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::Result;
use crate::link::LinkKind;
use crate::manifest::InstallMode;

pub const SCHEMA_VERSION: i64 = 1;

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

/// `paths::state_home()/my-skills/state.json`.
pub fn default_state_path() -> PathBuf {
    crate::paths::state_home()
        .join("my-skills")
        .join("state.json")
}

impl State {
    /// Missing file -> empty state. Errors (as `Error::State`) mirror v0.4.1:
    /// newer schema, non-object record, missing required fields. Unknown
    /// record fields are ignored.
    pub fn load(_path: Option<&Path>) -> Result<State> {
        todo!("stream B: spec §8.3")
    }

    /// Atomic write: temp file in the same directory, then rename.
    /// Records sorted by (skill, host); JSON indented by 2 with a trailing
    /// newline, as v0.4.1 writes it.
    pub fn save(&self) -> Result<()> {
        todo!("stream B: spec §8.3")
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
