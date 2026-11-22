//! Per-skill machine-local data directories. Stream B. Spec §8.4.
//! Mirrors v0.4.1 `data.py`.

use std::path::PathBuf;

use crate::Result;

/// `paths::data_root()/<skill>`. The name is checked with
/// `validate::validate_name` first, which also rejects path traversal.
/// With `create`, the directory is created; otherwise nothing is written.
pub fn skill_data_path(_skill: &str, _create: bool) -> Result<PathBuf> {
    todo!("stream B: spec §8.4")
}
