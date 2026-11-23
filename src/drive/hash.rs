//! Deterministic directory hashing. Stream A. Spec §8.5.
//!
//! Matches v0.4.1 `hashing.py` for regular files and directories, so existing
//! copy records keep their FRESH/STALE status after the switch (decision D6).

use std::fs::File;
use std::io::{self, Read};
use std::path::{MAIN_SEPARATOR, Path};

use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::drive::ignore::{IGNORED_DIRS, is_ignored};
use crate::{Error, Result};

/// Lowercase hex SHA-256 of one file's bytes (no prefix).
pub fn hash_file(path: &Path) -> Result<String> {
    let mut file = File::open(path).map_err(|err| Error::io(path, err))?;
    let mut hasher = Sha256::new();
    let mut chunk = [0; 64 * 1024];
    loop {
        match file.read(&mut chunk) {
            Ok(0) => break,
            Ok(size) => hasher.update(&chunk[..size]),
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(Error::io(path, err)),
        }
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// `"sha256:" + hex` over every regular file under `path`.
///
/// - Skip entries where `crate::drive::ignore::is_ignored(rel)` is true.
/// - Sort by the relative path with `/` separators, compared as UTF-8 bytes.
/// - For each file, feed `rel_path` UTF-8 bytes, `0x00`, `hash_file` hex
///   (ASCII), `0x00`.
/// - Skip links, including a symlink passed as the root.
pub fn hash_directory(path: &Path) -> Result<String> {
    // The ignored directory names exclude whole subtrees. Ignored file
    // names are checked separately, since a .DS_Store directory can hold files.
    let entries = WalkDir::new(path)
        .min_depth(1)
        .follow_links(false)
        .follow_root_links(false)
        .into_iter()
        .filter_entry(|entry| !IGNORED_DIRS.iter().any(|name| entry.file_name() == *name));

    let mut files = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|err| {
            let path = err.path().unwrap_or(path).to_path_buf();
            let source = err
                .into_io_error()
                .unwrap_or_else(|| io::Error::other("filesystem loop while walking directory"));
            Error::io(path, source)
        })?;
        if !entry.file_type().is_file() {
            continue;
        }
        let relative = entry
            .path()
            .strip_prefix(path)
            .expect("walkdir returns paths under its root");
        if is_ignored(relative) {
            continue;
        }
        let name = relative.to_str().ok_or_else(|| {
            Error::io(
                entry.path(),
                io::Error::new(io::ErrorKind::InvalidData, "file path is not valid UTF-8"),
            )
        })?;
        let name = if MAIN_SEPARATOR == '/' {
            name.to_owned()
        } else {
            name.replace(MAIN_SEPARATOR, "/")
        };
        files.push((name, entry.into_path()));
    }

    files.sort_unstable_by(|(left, _), (right, _)| left.cmp(right));
    let mut hasher = Sha256::new();
    for (name, file) in files {
        hasher.update(name.as_bytes());
        hasher.update(b"\0");
        hasher.update(hash_file(&file)?.as_bytes());
        hasher.update(b"\0");
    }
    Ok(format!("sha256:{:x}", hasher.finalize()))
}
