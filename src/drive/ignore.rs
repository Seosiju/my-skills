//! Entries skipped by hashing and copying. Must match v0.4.1 `ignore.py`
//! exactly, because the directory hash depends on it (spec §8.5).

use std::path::{Component, Path};

pub const IGNORED_DIRS: [&str; 3] = [".omc", ".git", "__pycache__"];
pub const IGNORED_FILES: [&str; 1] = [".DS_Store"];

/// True if `rel` (a path relative to a skill root) is skipped.
///
/// A path is skipped when its final name is an ignored file name, or when any
/// component is an ignored directory name.
pub fn is_ignored(rel: &Path) -> bool {
    if let Some(name) = rel.file_name().and_then(|n| n.to_str()) {
        if IGNORED_FILES.contains(&name) {
            return true;
        }
    }
    rel.components().any(|c| match c {
        Component::Normal(part) => part.to_str().is_some_and(|p| IGNORED_DIRS.contains(&p)),
        _ => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_ignored_names() {
        assert!(is_ignored(Path::new(".DS_Store")));
        assert!(is_ignored(Path::new("scripts/.DS_Store")));
        assert!(is_ignored(Path::new(".git/HEAD")));
        assert!(is_ignored(Path::new("scripts/__pycache__/x.pyc")));
        assert!(is_ignored(Path::new(".omc")));
    }

    #[test]
    fn keeps_regular_entries() {
        assert!(!is_ignored(Path::new("SKILL.md")));
        assert!(!is_ignored(Path::new("scripts/run.py")));
        assert!(!is_ignored(Path::new(".gitignore")));
    }
}
