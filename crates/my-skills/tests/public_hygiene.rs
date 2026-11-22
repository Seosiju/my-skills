//! Public-repository hygiene. Port of `tests/test_public_hygiene.py`, kept in
//! Rust so it survives removing the Python package (handoff §7 D), and widened
//! to Rust, shell, and PowerShell sources.
//!
//! Private values are assembled from pieces so this file never contains them.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const TEXT_SUFFIXES: &[&str] = &[
    "md", "json", "toml", "py", "yaml", "yml", "txt", "rs", "sh", "ps1", "lock", "",
];

fn repo_root() -> PathBuf {
    // crates/my-skills -> repository root. Adjust when the crate moves to the root.
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

fn tracked_files() -> Vec<String> {
    let output = Command::new("git")
        .arg("ls-files")
        .current_dir(repo_root())
        .output()
        .expect("git ls-files runs");
    assert!(output.status.success(), "git ls-files failed");
    String::from_utf8(output.stdout)
        .expect("tracked paths are UTF-8")
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

fn tracked_text() -> Vec<(String, String)> {
    tracked_files()
        .into_iter()
        .filter_map(|rel| {
            let path = repo_root().join(&rel);
            let suffix = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            // git ls-files lists unstaged deletions too.
            if !TEXT_SUFFIXES.contains(&suffix) || !path.is_file() {
                return None;
            }
            fs::read_to_string(&path).ok().map(|text| (rel, text))
        })
        .collect()
}

fn private_values() -> Vec<String> {
    vec![
        ["hermes", "forjs"].concat(),
        ["e0ffe", "317", "-5557-43b4-a551-466ff24275a2"].concat(),
        ["ideal", "innov39", "@", "gmail", ".com"].concat(),
        ["712", "020:", "076771c9-2a42-4120-877f-2b10b68c08bd"].concat(),
        ["jun", "seon"].concat(),
    ]
}

#[test]
fn tracked_files_do_not_contain_actual_private_jira_values() {
    let values = private_values();
    let leaks: Vec<String> = tracked_text()
        .iter()
        .flat_map(|(rel, text)| {
            values
                .iter()
                .filter(|value| text.contains(value.as_str()))
                .map(move |value| format!("{rel}: {value}"))
        })
        .collect();
    assert!(leaks.is_empty(), "{leaks:?}");
}

#[test]
fn tracked_files_do_not_contain_actual_machine_home_path() {
    let private_home = ["/Users/", "snu", ".sim"].concat();
    let leaks: Vec<String> = tracked_text()
        .into_iter()
        .filter(|(_, text)| text.contains(&private_home))
        .map(|(rel, _)| rel)
        .collect();
    assert!(leaks.is_empty(), "{leaks:?}");
}

#[test]
fn my_jira_uses_example_config_not_tracked_private_config() {
    let tracked: BTreeSet<String> = tracked_files().into_iter().collect();
    assert!(!tracked.contains("skills/my-jira/config.json"));
    assert!(tracked.contains("skills/my-jira/config.example.json"));
}

#[test]
fn cli_inventory_scratch_files_are_not_tracked() {
    let tracked: BTreeSet<String> = tracked_files().into_iter().collect();
    for scratch in [
        "skills/cli-inventory/cvelist",
        "skills/cli-inventory/cvelist-our",
        "skills/cli-inventory/cvelist.diff",
        "skills/cli-inventory/n",
        "skills/cli-inventory/patcheck.tmp",
    ] {
        assert!(!tracked.contains(scratch), "{scratch} is tracked");
    }
}

#[test]
fn roadmap_links_seed_design() {
    let roadmap =
        fs::read_to_string(repo_root().join("docs/2026-06-30-my-skills-open-source-roadmap.md"))
            .unwrap();
    assert!(roadmap.contains("docs/2026-07-01-default-skills-seed-design.md"));
    assert!(roadmap.contains("docs/2026-07-01-seed-implementation-spec.md"));
}
