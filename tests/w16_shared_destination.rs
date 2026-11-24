//! Single-host operations on a copy destination that two targets share.

mod support_h;

use std::fs;

use my_skills::drive::link::{self, EntryKind};
use my_skills::drive::state::State;
use my_skills::registry::manifest::InstallMode;
use predicates::prelude::predicate;
use serde_json::Value;
use support_h::{Fixture, skill};

/// `claude` and `codex` point at one host directory, both in copy mode, with
/// alpha installed to both.
fn shared() -> Fixture {
    let fixture = Fixture::repo();
    fixture.write_manifest("[defaults]\ninstall_mode = 'copy'\n[skills.alpha]\n[skills.beta]\n");
    for name in ["alpha", "beta"] {
        skill(
            &fixture.registry.join("skills"),
            name,
            "A synthetic test skill.",
            "Test body.",
        );
    }
    let claude = fixture.host("claude");
    fs::create_dir_all(&claude).unwrap();
    support_h::link_dir(&claude, &fixture.host("codex"));
    fixture
        .command()
        .args(["install", "alpha", "--yes"])
        .assert()
        .success();
    fixture
}

fn state_bytes(fixture: &Fixture) -> Vec<u8> {
    fs::read(fixture.state()).unwrap()
}

fn assert_untouched(fixture: &Fixture, state: &[u8]) {
    assert_eq!(state_bytes(fixture), state);
    let dest = fixture.host("claude").join("alpha");
    assert_eq!(link::entry_kind(&dest).unwrap(), EntryKind::Dir);
    let state = State::load(Some(&fixture.state())).unwrap();
    for host in ["claude", "codex"] {
        assert_eq!(state.get("alpha", host).unwrap().mode, InstallMode::Copy);
    }
}

fn statuses(fixture: &Fixture) -> Vec<(String, String, String)> {
    let output = fixture
        .command()
        .args(["status", "--json"])
        .assert()
        .success();
    let rows: Value = serde_json::from_slice(&output.get_output().stdout).unwrap();
    rows.as_array()
        .or_else(|| rows["skills"].as_array())
        .unwrap()
        .iter()
        .filter(|row| row["skill"] == "alpha")
        .map(|row| {
            (
                row["host"].as_str().unwrap().to_owned(),
                row["mode"].as_str().unwrap_or("").to_owned(),
                row["status"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

#[test]
fn converting_one_peer_is_blocked_before_anything_is_written() {
    let fixture = shared();
    let before = state_bytes(&fixture);
    fixture
        .command()
        .args(["convert", "alpha", "--to", "link", "--host", "claude"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("shared with alpha -> codex"));
    assert_untouched(&fixture, &before);

    fixture
        .command()
        .args(["convert", "alpha", "--to", "link", "--host", "claude"])
        .arg("--dry-run")
        .assert()
        .code(1)
        .stdout(predicate::str::contains("SHARED_DESTINATION"));
    assert_untouched(&fixture, &before);
    for (_, _, status) in statuses(&fixture) {
        assert_eq!(status, "FRESH");
    }
}

#[test]
fn uninstalling_one_peer_is_blocked_before_anything_is_written() {
    let fixture = shared();
    let before = state_bytes(&fixture);
    fixture
        .command()
        .args(["uninstall", "alpha", "--host", "claude"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("codex"));
    assert_untouched(&fixture, &before);

    fixture
        .command()
        .args(["uninstall", "alpha", "--host", "claude", "--dry-run"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("SHARED_DESTINATION"));
    assert_untouched(&fixture, &before);
}

#[test]
fn updating_one_peer_is_blocked_and_reported_in_the_dry_run_json() {
    let fixture = shared();
    fs::write(
        fixture.canonical("alpha").join("notes.md"),
        "A new reference.\n",
    )
    .unwrap();
    let before = state_bytes(&fixture);
    let preview = support_h::json(fixture.command().args([
        "sync",
        "alpha",
        "--host",
        "claude",
        "--dry-run",
        "--json",
    ]));
    assert_eq!(preview["blocked"], true, "{preview}");
    assert_eq!(preview["applicable"], false);
    let reason = preview["block_reasons"]
        .as_array()
        .unwrap()
        .iter()
        .find(|reason| reason["kind"] == "shared_destination")
        .unwrap_or_else(|| panic!("{preview}"));
    assert_eq!(reason["skill"], "alpha");
    assert_eq!(reason["host"], "claude");
    assert!(reason["message"].as_str().unwrap().contains("codex"));

    fixture
        .command()
        .args(["sync", "alpha", "--host", "claude", "--yes"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("codex"));
    assert_eq!(state_bytes(&fixture), before);
    assert!(!fixture.host("claude").join("alpha/notes.md").exists());

    // Both peers selected: the shared tree and both records change once.
    fixture
        .command()
        .args(["sync", "alpha", "--yes"])
        .assert()
        .success();
    assert!(fixture.host("claude").join("alpha/notes.md").is_file());
    for (_, _, status) in statuses(&fixture) {
        assert_eq!(status, "FRESH");
    }
}

#[test]
fn converting_both_peers_updates_the_shared_entry_and_both_records() {
    let fixture = shared();
    fixture
        .command()
        .args(["convert", "alpha", "--to", "link", "--yes"])
        .assert()
        .success();
    let dest = fixture.host("claude").join("alpha");
    assert!(matches!(
        link::entry_kind(&dest).unwrap(),
        EntryKind::Link(_)
    ));
    let state = State::load(Some(&fixture.state())).unwrap();
    for host in ["claude", "codex"] {
        assert_eq!(
            state.get("alpha", host).unwrap().mode,
            InstallMode::Link,
            "{host}: {}",
            String::from_utf8_lossy(&state_bytes(&fixture))
        );
    }
    for (host, mode, status) in statuses(&fixture) {
        assert_eq!(
            (mode.as_str(), status.as_str()),
            ("link", "FRESH"),
            "{host}"
        );
    }
}

#[test]
fn uninstalling_both_peers_removes_the_entry_and_both_records() {
    let fixture = shared();
    fixture
        .command()
        .args(["uninstall", "alpha", "--yes"])
        .assert()
        .success();
    assert!(!fixture.host("claude").join("alpha").exists());
    let state = State::load(Some(&fixture.state())).unwrap();
    assert_eq!(state.records().count(), 0);
}

#[test]
fn pruning_one_peer_keeps_the_shared_tree_for_the_peer_that_still_wants_it() {
    let fixture = shared();
    let text = fs::read_to_string(fixture.manifest())
        .unwrap()
        .replace("[skills.alpha]\n", "[skills.alpha]\nhosts = ['codex']\n");
    fs::write(fixture.manifest(), text).unwrap();
    fixture
        .command()
        .args(["sync", "alpha", "--yes"])
        .assert()
        .success();
    let dest = fixture.host("claude").join("alpha");
    assert_eq!(link::entry_kind(&dest).unwrap(), EntryKind::Dir);
    assert!(dest.join("SKILL.md").is_file());
    let state = State::load(Some(&fixture.state())).unwrap();
    assert!(state.get("alpha", "claude").is_none());
    assert_eq!(state.get("alpha", "codex").unwrap().mode, InstallMode::Copy);
    let rows = statuses(&fixture);
    let codex = rows.iter().find(|row| row.0 == "codex").unwrap();
    assert_eq!(codex.2, "FRESH");
}
