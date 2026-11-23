//! Directory links. Stream C (integrator). Spec §6.1, §7.2, invariants I1/I2/I4.
//!
//! This is the only module allowed to create or delete a host connection's
//! filesystem entry. Every other module asks it what an entry is.
//!
//! Safety rules enforced here:
//! - Entries are classified with `symlink_metadata`, never by following them.
//! - A link is removed as a link (`remove_file` / `remove_dir` on the link
//!   itself). This module never calls a recursive delete.
//! - On Windows a junction reports `is_symlink() == true` in Rust std, so the
//!   junction crate tells the two kinds apart.

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

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
pub fn entry_kind(path: &Path) -> Result<EntryKind> {
    let meta = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(EntryKind::Missing),
        Err(err) => return Err(Error::io(path, err)),
    };
    let file_type = meta.file_type();
    if file_type.is_symlink() {
        return Ok(EntryKind::Link(link_kind_of(path)?));
    }
    if file_type.is_dir() {
        Ok(EntryKind::Dir)
    } else {
        Ok(EntryKind::File)
    }
}

#[cfg(windows)]
fn link_kind_of(path: &Path) -> Result<LinkKind> {
    match junction::exists(path) {
        Ok(true) => Ok(LinkKind::Junction),
        Ok(false) => Ok(LinkKind::Symlink),
        Err(err) => Err(Error::io(path, err)),
    }
}

#[cfg(not(windows))]
fn link_kind_of(_path: &Path) -> Result<LinkKind> {
    Ok(LinkKind::Symlink)
}

/// Target of a symlink or junction, made absolute relative to the link's
/// parent. Errors if `path` is not a link.
pub fn link_target(path: &Path) -> Result<PathBuf> {
    let raw = match entry_kind(path)? {
        EntryKind::Link(LinkKind::Symlink) => {
            fs::read_link(path).map_err(|e| Error::io(path, e))?
        }
        EntryKind::Link(LinkKind::Junction) => junction_target(path)?,
        other => {
            return Err(Error::Link(format!(
                "{} is not a link ({other:?})",
                path.display()
            )));
        }
    };
    if raw.is_absolute() {
        return Ok(raw);
    }
    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    Ok(parent.join(raw))
}

#[cfg(windows)]
fn junction_target(path: &Path) -> Result<PathBuf> {
    junction::get_target(path).map_err(|e| Error::io(path, e))
}

#[cfg(not(windows))]
fn junction_target(path: &Path) -> Result<PathBuf> {
    Err(Error::Link(format!(
        "{}: junctions exist only on Windows",
        path.display()
    )))
}

/// True when `dest` is a symlink or junction whose target is `canonical`.
///
/// Both sides are compared after `canonicalize` when that succeeds, which
/// handles symlinked parents (macOS `/var` -> `/private/var`), case
/// differences on case-insensitive filesystems, and the Windows `\\?\`
/// prefix. When `canonical` no longer exists (a BROKEN connection, spec
/// §6.2), the link target and `canonical` are compared lexically instead, so
/// callers can still recognize their own dangling link.
pub fn is_managed_link(dest: &Path, canonical: &Path) -> bool {
    if !matches!(entry_kind(dest), Ok(EntryKind::Link(_))) {
        return false;
    }
    if let (Ok(resolved_dest), Ok(resolved_canonical)) =
        (fs::canonicalize(dest), fs::canonicalize(canonical))
    {
        return resolved_dest == resolved_canonical;
    }
    match link_target(dest) {
        Ok(target) => lexically_equal(&target, canonical),
        Err(_) => false,
    }
}

/// Compare two paths without touching the filesystem: `.` and `..` are
/// folded, the Windows verbatim prefix is ignored, and on Windows the
/// comparison is case-insensitive.
fn lexically_equal(a: &Path, b: &Path) -> bool {
    normalize_lexically(a) == normalize_lexically(b)
}

fn normalize_lexically(path: &Path) -> Vec<String> {
    let text = path.to_string_lossy();
    let text = text.strip_prefix(r"\\?\").unwrap_or(&text);
    let mut parts: Vec<String> = Vec::new();
    for component in Path::new(text).components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                parts.pop();
            }
            other => {
                let part = other.as_os_str().to_string_lossy().into_owned();
                parts.push(if cfg!(windows) {
                    part.to_lowercase()
                } else {
                    part
                });
            }
        }
    }
    parts
}

/// Create a link at `dest` pointing to the absolute `canonical`.
///
/// macOS/Linux: symlink. Windows: junction, then a directory symlink.
/// `dest` must not exist (refused otherwise, I3). `canonical` must be an
/// absolute, existing directory. Parent directories of `dest` are created.
/// Returns `NeedsCopy` instead of copying by itself, so the caller reports
/// the fallback (I4).
pub fn create_link(dest: &Path, canonical: &Path) -> Result<Created> {
    if !canonical.is_absolute() {
        return Err(Error::Link(format!(
            "link target must be absolute: {}",
            canonical.display()
        )));
    }
    if !canonical.is_dir() {
        return Err(Error::Link(format!(
            "link target is not a directory: {}",
            canonical.display()
        )));
    }
    match entry_kind(dest)? {
        EntryKind::Missing => {}
        other => {
            return Err(Error::Refused(format!(
                "I3: {} already exists ({other:?}); not replacing it",
                dest.display()
            )));
        }
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    create_platform_link(dest, canonical)
}

#[cfg(unix)]
fn create_platform_link(dest: &Path, canonical: &Path) -> Result<Created> {
    match std::os::unix::fs::symlink(canonical, dest) {
        Ok(()) => Ok(Created::Link(LinkKind::Symlink)),
        Err(err) => Ok(Created::NeedsCopy {
            reason: format!("symlink failed: {err}"),
        }),
    }
}

#[cfg(windows)]
fn create_platform_link(dest: &Path, canonical: &Path) -> Result<Created> {
    let junction_err = match junction::create(canonical, dest) {
        Ok(()) => return Ok(Created::Link(LinkKind::Junction)),
        Err(err) => err,
    };
    // A failed junction attempt can leave an empty directory behind.
    clear_empty_leftover(dest)?;
    let symlink_err = match std::os::windows::fs::symlink_dir(canonical, dest) {
        Ok(()) => return Ok(Created::Link(LinkKind::Symlink)),
        Err(err) => err,
    };
    clear_empty_leftover(dest)?;
    Ok(Created::NeedsCopy {
        reason: format!("junction failed: {junction_err}; symlink failed: {symlink_err}"),
    })
}

#[cfg(windows)]
fn clear_empty_leftover(dest: &Path) -> Result<()> {
    if entry_kind(dest)? == EntryKind::Dir {
        // remove_dir only succeeds on an empty directory, so this cannot
        // delete anything that was there before.
        fs::remove_dir(dest).map_err(|e| Error::io(dest, e))?;
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn create_platform_link(_dest: &Path, _canonical: &Path) -> Result<Created> {
    Ok(Created::NeedsCopy {
        reason: "links are not supported on this platform".into(),
    })
}

/// Remove the link entry only (I2). A missing entry is already removed and
/// returns `Ok`. A real directory or file is refused with `Error::Refused`.
/// Never calls a recursive delete.
pub fn remove_link(dest: &Path) -> Result<()> {
    match entry_kind(dest)? {
        EntryKind::Missing => Ok(()),
        EntryKind::Link(LinkKind::Junction) => fs::remove_dir(dest).map_err(|e| Error::io(dest, e)),
        EntryKind::Link(LinkKind::Symlink) => remove_symlink(dest),
        other => Err(Error::Refused(format!(
            "I2: {} is not a link ({other:?}); refusing to delete it",
            dest.display()
        ))),
    }
}

#[cfg(windows)]
fn remove_symlink(dest: &Path) -> Result<()> {
    // Directory symlinks are removed with remove_dir, file symlinks with
    // remove_file. Neither follows the link.
    fs::remove_dir(dest)
        .or_else(|_| fs::remove_file(dest))
        .map_err(|e| Error::io(dest, e))
}

#[cfg(not(windows))]
fn remove_symlink(dest: &Path) -> Result<()> {
    fs::remove_file(dest).map_err(|e| Error::io(dest, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canonical_with_files(root: &Path) -> PathBuf {
        let canonical = root.join("registry").join("skills").join("demo");
        fs::create_dir_all(canonical.join("scripts")).unwrap();
        fs::write(canonical.join("SKILL.md"), "---\nname: demo\n---\n").unwrap();
        fs::write(canonical.join("scripts").join("run.sh"), "echo hi\n").unwrap();
        canonical
    }

    fn assert_canonical_intact(canonical: &Path) {
        assert!(canonical.join("SKILL.md").is_file(), "I1: SKILL.md lost");
        assert!(
            canonical.join("scripts").join("run.sh").is_file(),
            "I1: nested file lost"
        );
    }

    #[test]
    fn missing_file_and_dir_are_classified() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(
            entry_kind(&tmp.path().join("nope")).unwrap(),
            EntryKind::Missing
        );
        fs::write(tmp.path().join("f"), "x").unwrap();
        assert_eq!(entry_kind(&tmp.path().join("f")).unwrap(), EntryKind::File);
        assert_eq!(entry_kind(tmp.path()).unwrap(), EntryKind::Dir);
    }

    #[test]
    fn create_then_recognize_link() {
        let tmp = tempfile::tempdir().unwrap();
        let canonical = canonical_with_files(tmp.path());
        let dest = tmp.path().join("host").join("skills").join("demo");

        let created = create_link(&dest, &canonical).unwrap();
        assert!(matches!(created, Created::Link(_)));
        assert!(matches!(entry_kind(&dest).unwrap(), EntryKind::Link(_)));
        assert!(is_managed_link(&dest, &canonical));
        assert!(
            dest.join("SKILL.md").is_file(),
            "host sees canonical content"
        );
    }

    #[test]
    fn link_to_another_dir_is_not_managed() {
        let tmp = tempfile::tempdir().unwrap();
        let canonical = canonical_with_files(tmp.path());
        let other = tmp.path().join("other");
        fs::create_dir_all(&other).unwrap();
        let dest = tmp.path().join("host").join("demo");
        create_link(&dest, &other).unwrap();
        assert!(!is_managed_link(&dest, &canonical));
    }

    #[test]
    fn real_dir_is_not_managed_link() {
        let tmp = tempfile::tempdir().unwrap();
        let canonical = canonical_with_files(tmp.path());
        let dest = tmp.path().join("host").join("demo");
        fs::create_dir_all(&dest).unwrap();
        assert!(!is_managed_link(&dest, &canonical));
    }

    #[test]
    fn create_refuses_existing_destination() {
        let tmp = tempfile::tempdir().unwrap();
        let canonical = canonical_with_files(tmp.path());
        let dest = tmp.path().join("host").join("demo");
        fs::create_dir_all(&dest).unwrap();
        fs::write(dest.join("mine.txt"), "user data").unwrap();

        let err = create_link(&dest, &canonical).unwrap_err();
        assert!(matches!(err, Error::Refused(_)), "{err}");
        assert_eq!(
            fs::read_to_string(dest.join("mine.txt")).unwrap(),
            "user data"
        );
    }

    #[test]
    fn create_rejects_relative_or_missing_target() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("host").join("demo");
        assert!(create_link(&dest, Path::new("relative/demo")).is_err());
        assert!(create_link(&dest, &tmp.path().join("absent")).is_err());
        assert_eq!(entry_kind(&dest).unwrap(), EntryKind::Missing);
    }

    #[test]
    fn remove_link_keeps_canonical_content() {
        let tmp = tempfile::tempdir().unwrap();
        let canonical = canonical_with_files(tmp.path());
        let dest = tmp.path().join("host").join("demo");
        create_link(&dest, &canonical).unwrap();

        remove_link(&dest).unwrap();
        assert_eq!(entry_kind(&dest).unwrap(), EntryKind::Missing);
        assert_canonical_intact(&canonical);
    }

    #[test]
    fn remove_link_refuses_real_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("host").join("demo");
        fs::create_dir_all(&dest).unwrap();
        fs::write(dest.join("SKILL.md"), "copy").unwrap();

        let err = remove_link(&dest).unwrap_err();
        assert!(matches!(err, Error::Refused(_)), "{err}");
        assert!(dest.join("SKILL.md").is_file());
    }

    #[test]
    fn remove_link_on_missing_is_ok() {
        let tmp = tempfile::tempdir().unwrap();
        remove_link(&tmp.path().join("gone")).unwrap();
    }

    #[test]
    fn dangling_link_to_canonical_is_still_recognized() {
        let tmp = tempfile::tempdir().unwrap();
        let canonical = canonical_with_files(tmp.path());
        let dest = tmp.path().join("host").join("demo");
        create_link(&dest, &canonical).unwrap();
        fs::remove_dir_all(&canonical).unwrap();

        assert!(matches!(entry_kind(&dest).unwrap(), EntryKind::Link(_)));
        assert!(
            is_managed_link(&dest, &canonical),
            "BROKEN detection needs this"
        );
        remove_link(&dest).unwrap();
        assert_eq!(entry_kind(&dest).unwrap(), EntryKind::Missing);
    }

    #[test]
    fn link_target_of_non_link_errors() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(link_target(tmp.path()).is_err());
    }

    #[test]
    fn lexical_normalization() {
        assert!(lexically_equal(
            Path::new("/a/b/../c/./d"),
            Path::new("/a/c/d")
        ));
        assert!(!lexically_equal(Path::new("/a/b"), Path::new("/a/c")));
    }

    #[cfg(unix)]
    mod unix {
        use super::*;

        #[test]
        fn symlink_kind_and_relative_target() {
            let tmp = tempfile::tempdir().unwrap();
            let canonical = canonical_with_files(tmp.path());
            let host = tmp.path().join("host");
            fs::create_dir_all(&host).unwrap();
            let dest = host.join("demo");
            std::os::unix::fs::symlink("../registry/skills/demo", &dest).unwrap();

            assert_eq!(
                entry_kind(&dest).unwrap(),
                EntryKind::Link(LinkKind::Symlink)
            );
            assert!(is_managed_link(&dest, &canonical));
            assert!(lexically_equal(&link_target(&dest).unwrap(), &canonical));
        }

        #[test]
        fn symlinked_parent_paths_compare_equal() {
            // macOS temp dirs live under /var -> /private/var; compare via a
            // symlinked alias of the registry root as well.
            let tmp = tempfile::tempdir().unwrap();
            let canonical = canonical_with_files(tmp.path());
            let alias = tmp.path().join("alias");
            std::os::unix::fs::symlink(tmp.path().join("registry"), &alias).unwrap();
            let via_alias = alias.join("skills").join("demo");

            let dest = tmp.path().join("host").join("demo");
            create_link(&dest, &via_alias).unwrap();
            assert!(is_managed_link(&dest, &canonical));
        }

        #[test]
        fn file_symlink_is_removed_without_touching_target() {
            let tmp = tempfile::tempdir().unwrap();
            let target = tmp.path().join("target.txt");
            fs::write(&target, "keep").unwrap();
            let dest = tmp.path().join("link.txt");
            std::os::unix::fs::symlink(&target, &dest).unwrap();
            remove_link(&dest).unwrap();
            assert_eq!(fs::read_to_string(&target).unwrap(), "keep");
        }
    }

    #[cfg(windows)]
    mod windows {
        use super::*;

        #[test]
        fn create_link_prefers_junction() {
            let tmp = tempfile::tempdir().unwrap();
            let canonical = canonical_with_files(tmp.path());
            let dest = tmp.path().join("host").join("demo");
            assert_eq!(
                create_link(&dest, &canonical).unwrap(),
                Created::Link(LinkKind::Junction)
            );
            assert_eq!(
                entry_kind(&dest).unwrap(),
                EntryKind::Link(LinkKind::Junction)
            );
            assert!(is_managed_link(&dest, &canonical));
        }

        #[test]
        fn junction_removal_keeps_canonical() {
            let tmp = tempfile::tempdir().unwrap();
            let canonical = canonical_with_files(tmp.path());
            let dest = tmp.path().join("host").join("demo");
            create_link(&dest, &canonical).unwrap();
            remove_link(&dest).unwrap();
            assert_eq!(entry_kind(&dest).unwrap(), EntryKind::Missing);
            assert_canonical_intact(&canonical);
        }

        #[test]
        fn junction_target_reads_back() {
            let tmp = tempfile::tempdir().unwrap();
            let canonical = canonical_with_files(tmp.path());
            let dest = tmp.path().join("host").join("demo");
            create_link(&dest, &canonical).unwrap();
            assert!(lexically_equal(&link_target(&dest).unwrap(), &canonical));
        }

        #[test]
        fn case_and_verbatim_prefix_are_ignored() {
            let tmp = tempfile::tempdir().unwrap();
            let canonical = canonical_with_files(tmp.path());
            let dest = tmp.path().join("host").join("demo");
            create_link(&dest, &canonical).unwrap();
            let upper = PathBuf::from(canonical.to_string_lossy().to_uppercase());
            assert!(is_managed_link(&dest, &upper));
            let verbatim = PathBuf::from(format!(r"\\?\{}", canonical.display()));
            assert!(lexically_equal(&verbatim, &canonical));
        }
    }
}
