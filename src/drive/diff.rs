use std::collections::BTreeMap;
use std::fmt::Write;
use std::fs::File;
use std::io::{self, Read};
use std::path::{MAIN_SEPARATOR, Path, PathBuf};

use serde::Serialize;
use similar::TextDiff;
use walkdir::WalkDir;

use crate::drive::hash::hash_file;
use crate::drive::ignore::{IGNORED_DIRS, is_ignored};
use crate::{Error, Result};

const MAX_PATCH_BYTES: u64 = 1024 * 1024;

#[derive(Default, Serialize)]
pub struct FileChanges {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub modified: Vec<String>,
}

impl FileChanges {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.modified.is_empty()
    }
}

struct FileEntry {
    path: PathBuf,
    hash: String,
}

/// Compare regular files using the same inclusion rules and per-file SHA-256
/// as the drive's directory hash. Added and removed are relative to canonical.
pub fn compare(
    canonical: &Path,
    installed: &Path,
    include_patch: bool,
) -> Result<(FileChanges, Option<String>)> {
    let source = if canonical.is_dir() {
        inventory(canonical)?
    } else {
        BTreeMap::new()
    };
    let dest = inventory(installed)?;
    let mut changes = FileChanges::default();
    for (name, entry) in &source {
        match dest.get(name) {
            None => changes.removed.push(name.clone()),
            Some(other) if entry.hash != other.hash => changes.modified.push(name.clone()),
            Some(_) => {}
        }
    }
    changes.added.extend(
        dest.keys()
            .filter(|name| !source.contains_key(*name))
            .cloned(),
    );

    let patch = if include_patch && !changes.is_empty() {
        let mut patch = String::new();
        for name in changes
            .added
            .iter()
            .chain(&changes.removed)
            .chain(&changes.modified)
        {
            append_patch(&mut patch, name, source.get(name), dest.get(name))?;
        }
        Some(patch)
    } else {
        None
    };
    Ok((changes, patch))
}

fn inventory(root: &Path) -> Result<BTreeMap<String, FileEntry>> {
    // As in hash_directory, ignore a named directory's entire subtree, but
    // keep walking a real directory named after an ignored file.
    let entries = WalkDir::new(root)
        .min_depth(1)
        .follow_links(false)
        .follow_root_links(false)
        .into_iter()
        .filter_entry(|entry| !IGNORED_DIRS.iter().any(|name| entry.file_name() == *name));

    let mut files = BTreeMap::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            let path = error.path().unwrap_or(root).to_path_buf();
            Error::io(path, io::Error::from(error))
        })?;
        if !entry.file_type().is_file() {
            continue;
        }
        let relative = entry
            .path()
            .strip_prefix(root)
            .expect("entry is under the root");
        if is_ignored(relative) {
            continue;
        }
        let name = relative.to_str().ok_or_else(|| {
            Error::io(
                entry.path(),
                io::Error::new(io::ErrorKind::InvalidData, "file path is not valid UTF-8"),
            )
        })?;
        let name = name.replace(MAIN_SEPARATOR, "/");
        let path = entry.into_path();
        let hash = hash_file(&path)?;
        files.insert(name, FileEntry { path, hash });
    }
    Ok(files)
}

enum PatchContents {
    Text(String),
    Binary,
    Large,
}

impl PatchContents {
    fn read(file: Option<&FileEntry>) -> Result<Self> {
        let Some(file) = file else {
            return Ok(Self::Text(String::new()));
        };
        let path = &file.path;
        let mut bytes = Vec::new();
        File::open(path)
            .and_then(|file| file.take(MAX_PATCH_BYTES + 1).read_to_end(&mut bytes))
            .map_err(|error| Error::io(path, error))?;
        Ok(if bytes.len() as u64 > MAX_PATCH_BYTES {
            Self::Large
        } else if bytes.contains(&0) {
            Self::Binary
        } else {
            String::from_utf8(bytes).map_or(Self::Binary, Self::Text)
        })
    }
}

fn append_patch(
    patch: &mut String,
    name: &str,
    source: Option<&FileEntry>,
    dest: Option<&FileEntry>,
) -> Result<()> {
    let before = PatchContents::read(source)?;
    let after = PatchContents::read(dest)?;
    let note = match (before, after) {
        (PatchContents::Large, _) | (_, PatchContents::Large) => "file exceeds 1 MiB",
        (PatchContents::Binary, _) | (_, PatchContents::Binary) => "binary file",
        (PatchContents::Text(before), PatchContents::Text(after)) => {
            if before.is_empty() && after.is_empty() {
                if source.is_none() {
                    "empty file added"
                } else {
                    "empty file removed"
                }
            } else {
                let old_name = patch_filename("a", name, source.is_some());
                let new_name = patch_filename("b", name, dest.is_some());
                let diff = TextDiff::from_lines(&before, &after);
                write!(
                    patch,
                    "{}",
                    diff.unified_diff().header(&old_name, &new_name)
                )
                .expect("writing to a String cannot fail");
                return Ok(());
            }
        }
    };
    writeln!(patch, "# {name}: {note}").expect("writing to a String cannot fail");
    Ok(())
}

fn patch_filename(prefix: &str, name: &str, exists: bool) -> String {
    if !exists {
        return "/dev/null".to_owned();
    }
    let path = format!("{prefix}/{name}");
    if path.chars().any(char::is_control) {
        format!("{path:?}")
    } else {
        path
    }
}
