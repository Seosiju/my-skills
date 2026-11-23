//! Per-skill machine-local data directories. Stream B. Spec §8.4.
//! Mirrors v0.4.1 `data.py`.

use std::fs;
use std::path::PathBuf;

use crate::{Error, Result, check::validate, platform::paths};

/// `paths::data_root()/<skill>`. The name is checked with
/// `validate::validate_name` first, which also rejects path traversal.
/// With `create`, the directory is created; otherwise nothing is written.
pub fn skill_data_path(skill: &str, create: bool) -> Result<PathBuf> {
    let errors = validate::validate_name(skill);
    if !errors.is_empty() {
        return Err(Error::Refused(format!(
            "invalid skill name {skill:?}: {}",
            errors.join("; ")
        )));
    }
    let path = paths::data_root().join(skill);
    if create {
        fs::create_dir_all(&path).map_err(|error| Error::io(&path, error))?;
    }
    Ok(path)
}
