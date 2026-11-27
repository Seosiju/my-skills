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
        .args(["sync", "alpha", "--yes"])
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
        .args(["skills", "--json"])
        .assert()
        .success();
    let rows: Value = serde_json::from_slice(&output.get_output().stdout).unwrap();
    let alpha = rows["skills"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["name"] == "alpha")
        .unwrap();
    let state = State::load(Some(&fixture.state())).unwrap();
    state
        .records()
        .filter(|record| record.skill == "alpha")
        .map(|record| {
            (
                record.host.clone(),
                serde_json::to_value(record.mode)
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_owned(),
                alpha["status"][&record.host].as_str().unwrap().to_owned(),
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
        .args(["sync", "alpha", "--mode", "link", "--host", "claude"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("shared with alpha -> codex"));
    assert_untouched(&fixture, &before);

    fixture
        .command()
        .args(["sync", "alpha", "--mode", "link", "--host", "claude"])
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(predicate::str::contains("SHARED_DESTINATION"));
    assert_untouched(&fixture, &before);
    for (_, _, status) in statuses(&fixture) {
        assert_eq!(status, "FRESH");
    }
}

#[test]
fn pruning_one_of_two_unselected_peers_is_blocked_before_anything_is_written() {
    let fixture = shared();
    fixture
        .command()
        .args(["disable", "alpha"])
        .assert()
        .success();
    let before = state_bytes(&fixture);
    fixture
        .command()
        .args(["sync", "alpha", "--host", "claude"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("codex"));
    assert_untouched(&fixture, &before);

    fixture
        .command()
        .args(["sync", "alpha", "--host", "claude", "--dry-run"])
        .assert()
        .success()
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
        .args(["sync", "alpha", "--mode", "link", "--yes"])
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
fn pruning_both_peers_removes_the_entry_and_both_records() {
    let fixture = shared();
    fixture
        .command()
        .args(["disable", "alpha"])
        .assert()
        .success();
    fixture
        .command()
        .args(["sync", "alpha", "--yes"])
        .assert()
        .success();
    assert!(!fixture.host("claude").join("alpha").exists());
    let state = State::load(Some(&fixture.state())).unwrap();
    assert_eq!(state.records().count(), 0);
}

#[test]
fn different_modes_block_only_the_shared_group() {
    let fixture = shared();
    let text = fs::read_to_string(fixture.manifest())
        .unwrap()
        .replace("[skills.beta]\n", "[skills.beta]\nhosts = ['codex']\n");
    fs::write(fixture.manifest(), text).unwrap();
    fs::write(
        fixture.registry.join("my-skills.local.toml"),
        "[targets.claude]\ninstall_mode = 'link'\n",
    )
    .unwrap();
    let state_before = State::load(Some(&fixture.state())).unwrap();
    let installed_alpha = fixture.host("claude").join("alpha/SKILL.md");
    let alpha_before = fs::read(&installed_alpha).unwrap();

    fixture
        .command()
        .args(["sync", "--check"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains(
            "  claude   BLOCKED (hosts sharing",
        ))
        .stdout(predicate::str::contains(
            "  codex    BLOCKED (hosts sharing",
        ));
    let applied = fixture
        .command()
        .args(["sync", "--yes", "--json"])
        .assert()
        .code(1);
    let report: Value = serde_json::from_slice(&applied.get_output().stdout).unwrap();
    for row in report["actions"].as_array().unwrap() {
        if row["skill"] == "alpha" {
            assert_eq!(row["action"], "BLOCK_DRIFT");
            assert_eq!(row["result"], "blocked");
            assert!(
                row["reason"]
                    .as_str()
                    .unwrap()
                    .ends_with("want different install modes")
            );
        } else {
            assert_eq!(row["skill"], "beta");
            assert_eq!(row["host"], "codex");
            assert_eq!(row["result"], "created");
        }
    }
    assert_eq!(report["summary"]["blocked"], 2);
    assert_eq!(report["summary"]["created"], 1);
    let state_after = State::load(Some(&fixture.state())).unwrap();
    for host in ["claude", "codex"] {
        assert_eq!(
            state_after.get("alpha", host),
            state_before.get("alpha", host)
        );
    }
    assert_eq!(fs::read(installed_alpha).unwrap(), alpha_before);
    assert!(fixture.host("codex").join("beta/SKILL.md").is_file());
    assert!(state_after.get("beta", "codex").is_some());
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
