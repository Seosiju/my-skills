use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;

use my_skills::Error;
use my_skills::hash::{hash_directory, hash_file};
use serde::Deserialize;
use tempfile::tempdir;

#[derive(Deserialize)]
struct Case {
    name: String,
    files: BTreeMap<String, String>,
    dirs: Vec<String>,
}

#[test]
fn directory_hashes_match_python_v0_4_1() {
    let cases: Vec<Case> = serde_json::from_str(include_str!("golden/hash/cases.json")).unwrap();
    let mut golden: BTreeMap<String, String> =
        serde_json::from_str(include_str!("golden/hash/golden.json")).unwrap();
    let temp = tempdir().unwrap();

    for case in cases {
        let root = temp.path().join(&case.name);
        fs::create_dir(&root).unwrap();
        for dir in case.dirs {
            fs::create_dir_all(root.join(dir)).unwrap();
        }
        for (relative_path, encoded) in case.files.into_iter().rev() {
            let path = root.join(relative_path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, decode_base64(&encoded)).unwrap();
        }

        let expected = golden
            .remove(&case.name)
            .unwrap_or_else(|| panic!("missing golden hash for {}", case.name));
        assert_eq!(hash_directory(&root).unwrap(), expected, "{}", case.name);
    }
    assert!(golden.is_empty(), "unused golden hashes: {golden:?}");
}

#[test]
fn file_hash_is_lowercase_hex_without_a_prefix() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("file.txt");
    fs::write(&path, b"abc").unwrap();

    assert_eq!(
        hash_file(&path).unwrap(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn missing_paths_are_io_errors() {
    let temp = tempdir().unwrap();
    let missing = temp.path().join("missing");
    for result in [hash_file(&missing), hash_directory(&missing)] {
        let Error::Io { path, source } = result.unwrap_err() else {
            panic!("expected an I/O error for a missing path");
        };
        assert_eq!(path, missing);
        assert_eq!(source.kind(), ErrorKind::NotFound);
    }
}

#[cfg(unix)]
#[test]
fn directory_hash_skips_symlinks() {
    use std::os::unix::fs::symlink;

    let temp = tempdir().unwrap();
    let root = temp.path().join("root");
    let external = temp.path().join("external");
    fs::create_dir(&root).unwrap();
    fs::create_dir(&external).unwrap();
    fs::write(root.join("SKILL.md"), b"# Fixture\n").unwrap();
    fs::write(external.join("file.txt"), b"external").unwrap();
    let expected = hash_directory(&root).unwrap();

    symlink(&external, root.join("linked-dir")).unwrap();
    symlink(external.join("file.txt"), root.join("linked-file")).unwrap();
    assert_eq!(hash_directory(&root).unwrap(), expected);

    let linked_root = temp.path().join("linked-root");
    symlink(&root, &linked_root).unwrap();
    let empty_root = temp.path().join("empty");
    fs::create_dir(&empty_root).unwrap();
    assert_eq!(
        hash_directory(&linked_root).unwrap(),
        hash_directory(&empty_root).unwrap()
    );
}

// macOS filesystems reject invalid UTF-8 names before hashing can see them.
#[cfg(target_os = "linux")]
#[test]
fn non_utf8_relative_path_is_an_io_error() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let temp = tempdir().unwrap();
    let path = temp.path().join(OsStr::from_bytes(b"bad-name-\xff"));
    fs::write(&path, b"content").unwrap();

    let err = hash_directory(temp.path()).unwrap_err();
    assert!(err.to_string().contains("bad-name-"));
    assert!(err.to_string().contains("UTF-8"));
    let Error::Io {
        path: error_path,
        source,
    } = err
    else {
        panic!("expected an I/O error for a non-UTF-8 path");
    };
    assert_eq!(error_path, path);
    assert_eq!(source.kind(), ErrorKind::InvalidData);
}

fn decode_base64(encoded: &str) -> Vec<u8> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let sextet = |byte| {
        ALPHABET
            .iter()
            .position(|&digit| digit == byte)
            .expect("invalid base64 fixture") as u8
    };
    assert_eq!(encoded.len() % 4, 0, "expected padded base64");
    let mut bytes = Vec::with_capacity(encoded.len() / 4 * 3);
    for quad in encoded.as_bytes().chunks_exact(4) {
        let a = sextet(quad[0]);
        let b = sextet(quad[1]);
        bytes.push((a << 2) | (b >> 4));
        if quad[2] != b'=' {
            let c = sextet(quad[2]);
            bytes.push((b << 4) | (c >> 2));
            if quad[3] != b'=' {
                bytes.push((c << 6) | sextet(quad[3]));
            }
        }
    }
    bytes
}
