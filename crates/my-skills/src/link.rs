//! Directory links. Stream C (integrator). Spec §6.1, §7.2, invariants I1/I2/I4.
//!
//! This is the only module allowed to create or delete a host connection's
//! filesystem entry. Every other module asks it what an entry is.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LinkKind {
    Symlink,
    Junction,
}

/// What sits at a path, read without following links.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    Link(LinkKind),
    Dir,
    File,
    Missing,
}

/// Result of `create_link`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Created {
    Link(LinkKind),
    /// No link could be made; the caller must copy and report `reason` (I4).
    NeedsCopy {
        reason: String,
    },
}

/// Classify `path` from `symlink_metadata`. Never follows the entry.
pub fn entry_kind(_path: &Path) -> Result<EntryKind> {
    todo!("stream C: spec §6.1")
}

/// Target of a symlink or junction, made absolute relative to the link's
/// parent. Errors if `path` is not a link.
pub fn link_target(_path: &Path) -> Result<PathBuf> {
    todo!("stream C: spec §6.1")
}

/// True when `dest` is a symlink or junction whose normalized target equals
/// the normalized `canonical` (case-insensitive where the filesystem is,
/// `\\?\` stripped on Windows).
pub fn is_managed_link(_dest: &Path, _canonical: &Path) -> bool {
    todo!("stream C: spec §6.1")
}

/// Create a link at `dest` pointing to the absolute `canonical`.
/// macOS/Linux: symlink. Windows: junction, then symlink. `dest` must not
/// exist. Returns `NeedsCopy` instead of copying by itself.
pub fn create_link(_dest: &Path, _canonical: &Path) -> Result<Created> {
    todo!("stream C: spec §6.1, §7.2")
}

/// Remove the link entry only (I2). Errors with `Error::Refused` if `dest`
/// is not a link. Never calls a recursive delete.
pub fn remove_link(_dest: &Path) -> Result<()> {
    todo!("stream C: spec §6.1, §4 I2")
}
