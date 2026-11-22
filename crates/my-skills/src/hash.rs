//! Deterministic directory hashing. Stream A. Spec §8.5.
//!
//! Must be byte-for-byte identical to v0.4.1 `hashing.py`, so existing copy
//! records keep their FRESH/STALE status after the switch (decision D6).

use std::path::Path;

use crate::Result;

/// Lowercase hex SHA-256 of one file's bytes (no prefix).
pub fn hash_file(_path: &Path) -> Result<String> {
    todo!("stream A: spec §8.5")
}

/// `"sha256:" + hex` over every regular file under `path`.
///
/// - Skip entries where `crate::ignore::is_ignored(rel)` is true.
/// - Sort by the relative path with `/` separators, compared as UTF-8 bytes.
/// - For each file, feed `rel_path` UTF-8 bytes, `0x00`, `hash_file` hex
///   (ASCII), `0x00`.
/// - Do not follow directory links while walking (spec §4 I2). v0.4.1 used
///   `rglob` + `is_file()`; import already rejects symlinks inside skills, so
///   the results agree for every real registry. Stream A documents any
///   divergence it finds in tests.
pub fn hash_directory(_path: &Path) -> Result<String> {
    todo!("stream A: spec §8.5")
}
