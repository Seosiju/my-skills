mod support_h;

use std::fs;

use my_skills::drive::link::{self, EntryKind};
use predicates::str::contains;
use serde_json::Value;
use support_h::{Fixture, skill};

fn repo() -> Fixture {
    let fixture = Fixture::repo();
    fixture.write_manifest(
        "[defaults]\ninstall_mode = 'link'\n\
         [skills.alpha]\ngroup = 'test-batch'\n[skills.beta]\ngroup = 'test-batch'\n",
    );
    for name in ["alpha", "beta"] {
        skill(
            &fixture.registry.join("skills"),
            name,
            "A synthetic test skill.",
            "Test body.",
        );
    }
    fixture
        .command()
        .args(["install", "alpha", "--host", "claude", "--yes"])
        .assert()
        .success();
    assert!(matches!(
        link::entry_kind(&fixture.host("claude").join("alpha")).unwrap(),
        EntryKind::Link(_)
    ));
    fixture
}

fn break_alpha(fixture: &Fixture) {
    fs::remove_file(fixture.canonical("alpha").join("SKILL.md")).unwrap();
}

#[test]
fn disabled_broken_skill_can_be_pruned_by_explicit_sync() {
    let fixture = repo();
    break_alpha(&fixture);
    fixture
        .command()
        .args(["disable", "alpha"])
        .assert()
        .success();
    fixture
        .command()
        .args(["sync", "alpha", "--host", "claude", "--yes"])
        .assert()
        .success()
        .stdout(contains("removed: alpha"));
    assert!(!fixture.host("claude").join("alpha").exists());
    assert!(fixture.canonical("alpha").exists());
}

#[test]
fn explicit_sync_checks_a_fresh_link_even_with_no_writes() {
    let fixture = repo();
    break_alpha(&fixture);
    let state = fs::read(fixture.state()).unwrap();
    fixture
        .command()
        .args(["sync", "alpha", "--host", "claude", "--yes"])
        .assert()
        .code(1)
        .stdout(contains("[BLOCKED] alpha: validation failed"));
    let preview = fixture
        .command()
        .args(["sync", "alpha", "--host", "claude", "--dry-run", "--json"])
        .assert()
        .code(1);
    let json: Value = serde_json::from_slice(&preview.get_output().stdout).unwrap();
    assert_eq!(json["blocked"], true);
    assert_eq!(json["applicable"], false);
    assert_eq!(json["actions"][0]["action"], "NOOP");
    assert_eq!(json["block_reasons"][0]["kind"], "validation");
    assert_eq!(json["block_reasons"][0]["skill"], "alpha");
    assert_eq!(json["validation"][0]["ok"], false);
    assert_eq!(
        json["validation"][0]["errors"],
        serde_json::json!(["SKILL.md not found"])
    );
    assert_eq!(state, fs::read(fixture.state()).unwrap());
    assert!(!fixture.host("claude").join("alpha/SKILL.md").exists());
    assert!(matches!(
        link::entry_kind(&fixture.host("claude").join("alpha")).unwrap(),
        EntryKind::Link(_)
    ));
}

#[test]
fn bare_sync_keeps_its_previous_scope() {
    let fixture = repo();
    break_alpha(&fixture);
    fixture
        .command()
        .args(["sync", "--host", "claude", "--yes"])
        .assert()
        .success()
        .stdout(contains("unchanged: alpha"))
        .stdout(contains("created: beta"));
}

#[test]
fn group_validation_failure_prevents_a_good_skills_copy_update() {
    let fixture = repo();
    fixture
        .command()
        .args(["install", "beta", "--host", "claude", "--mode", "copy"])
        .assert()
        .success();
    let state = fs::read(fixture.state()).unwrap();
    let installed = fixture.host("claude").join("beta/SKILL.md");
    let old_copy = fs::read(&installed).unwrap();
    skill(
        &fixture.registry.join("skills"),
        "beta",
        "A synthetic test skill.",
        "An update.",
    );
    break_alpha(&fixture);
    fixture
        .command()
        .args(["sync", "--group", "test-batch", "--host", "claude", "--yes"])
        .assert()
        .code(1)
        .stdout(contains("[BLOCKED] alpha: validation failed"));
    assert_eq!(fs::read(&installed).unwrap(), old_copy);
    assert_eq!(state, fs::read(fixture.state()).unwrap());
    assert_eq!(fs::read_dir(fixture.host("claude")).unwrap().count(), 2);
}

#[test]
fn explicit_sync_validates_fresh_links_across_hosts_without_yes() {
    let fixture = repo();
    fixture
        .command()
        .args(["install", "alpha", "--host", "codex", "--yes"])
        .assert()
        .success();
    break_alpha(&fixture);
    let state = fs::read(fixture.state()).unwrap();
    fixture
        .command()
        .args(["sync", "alpha"])
        .assert()
        .code(1)
        .stdout(contains("SKILL.md not found"));
    let preview = fixture
        .command()
        .args(["sync", "alpha", "--dry-run", "--json"])
        .assert()
        .code(1);
    let preview: Value = serde_json::from_slice(&preview.get_output().stdout).unwrap();
    assert_eq!(preview["blocked"], true);
    assert_eq!(preview["applicable"], false);
    assert_eq!(preview["block_reasons"][0]["kind"], "validation");
    assert_eq!(state, fs::read(fixture.state()).unwrap());
}

#[test]
fn converting_a_live_link_to_copy_checks_the_snapshot_before_replacing_it() {
    let fixture = repo();
    break_alpha(&fixture);
    let state = fs::read(fixture.state()).unwrap();
    fixture
        .command()
        .args([
            "convert",
            "alpha",
            "--to",
            "copy",
            "--host",
            "claude",
            "--dry-run",
        ])
        .assert()
        .code(1)
        .stdout(contains("[BLOCKED] alpha: validation failed"))
        .stdout(contains("SKILL.md not found"));
    assert_eq!(state, fs::read(fixture.state()).unwrap());
    assert!(matches!(
        link::entry_kind(&fixture.host("claude").join("alpha")).unwrap(),
        EntryKind::Link(_)
    ));
    fixture
        .command()
        .args([
            "convert", "alpha", "--to", "copy", "--host", "claude", "--yes",
        ])
        .assert()
        .code(1)
        .stdout(contains("[BLOCKED] alpha: validation failed"));
    assert_eq!(state, fs::read(fixture.state()).unwrap());
    assert!(matches!(
        link::entry_kind(&fixture.host("claude").join("alpha")).unwrap(),
        EntryKind::Link(_)
    ));
}

#[test]
fn sync_previews_validate_the_files_the_copy_will_keep() {
    let fixture = repo();
    let source = skill(
        &fixture.registry.join("skills"),
        "beta",
        "A fixture skill.",
        "[Supporting note](__pycache__/fixture.md)",
    );
    fs::create_dir(source.join("__pycache__")).unwrap();
    fs::write(source.join("__pycache__/fixture.md"), "Fixture note.").unwrap();
    let manifest = fixture.manifest();
    let text = fs::read_to_string(&manifest)
        .unwrap()
        .replace("install_mode = 'link'", "install_mode = 'copy'");
    fs::write(manifest, text).unwrap();
    let before = fs::read(fixture.state()).unwrap();

    let preview = fixture
        .command()
        .args(["sync", "beta", "--host", "claude", "--dry-run", "--json"])
        .assert()
        .success();
    let report: Value = serde_json::from_slice(&preview.get_output().stdout).unwrap();
    assert_eq!(report["blocked"], false);
    assert_eq!(report["applicable"], true);
    assert_eq!(report["block_reasons"], serde_json::json!([]));
    assert!(
        report["validation"][0]["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|warning| warning.as_str().unwrap().contains("__pycache__/fixture.md"))
    );
    let destination = fixture.host("claude").join("beta");
    assert!(!destination.exists());
    assert_eq!(fs::read(fixture.state()).unwrap(), before);
    fixture
        .command()
        .args(["sync", "beta", "--host", "claude", "--yes"])
        .assert()
        .success()
        .stdout(contains("__pycache__/fixture.md").count(1));
    assert_eq!(
        fs::read(destination.join("SKILL.md")).unwrap(),
        fs::read(source.join("SKILL.md")).unwrap()
    );
    assert!(!destination.join("__pycache__").exists());
    assert_ne!(fs::read(fixture.state()).unwrap(), before);
}

#[test]
fn mixed_modes_report_validation_once_per_skill() {
    let fixture = repo();
    fixture
        .command()
        .args(["install", "alpha", "--host", "codex", "--mode", "copy"])
        .assert()
        .success();
    let state = fs::read(fixture.state()).unwrap();
    break_alpha(&fixture);
    fixture
        .command()
        .args(["sync", "alpha", "--yes"])
        .assert()
        .code(1)
        .stdout(contains("[BLOCKED] alpha: validation failed").count(1));
    let preview = fixture
        .command()
        .args(["sync", "alpha", "--dry-run", "--json", "--yes"])
        .assert()
        .code(1);
    let json: Value = serde_json::from_slice(&preview.get_output().stdout).unwrap();
    // Host rows that repeat the skill-level error are dropped, so the failure
    // appears once however many hosts or modes are involved.
    let failing = |rows: &Value| {
        rows.as_array()
            .unwrap()
            .iter()
            .filter(|row| row["ok"] != true)
            .map(|row| row["host"].clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(failing(&json["validation"]), [Value::Null]);
    assert_eq!(
        json["block_reasons"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["kind"] == "validation")
            .count(),
        1
    );
    assert_eq!(fs::read(fixture.state()).unwrap(), state);
}

#[test]
fn convert_to_copy_preserves_skill_content_with_remote_image() {
    let fixture = repo();
    let before = fs::read(fixture.state()).unwrap();
    skill(
        &fixture.registry.join("skills"),
        "alpha",
        "A fixture skill.",
        "![Remote](https://example.invalid/fixture.png)",
    );
    fixture
        .command()
        .args([
            "convert",
            "alpha",
            "--to",
            "copy",
            "--host",
            "claude",
            "--dry-run",
        ])
        .assert()
        .success();
    assert_eq!(fs::read(fixture.state()).unwrap(), before);
    fixture
        .command()
        .args([
            "convert", "alpha", "--to", "copy", "--host", "claude", "--yes",
        ])
        .assert()
        .success();
    assert_eq!(
        fs::read(fixture.host("claude").join("alpha/SKILL.md")).unwrap(),
        fs::read(fixture.canonical("alpha").join("SKILL.md")).unwrap()
    );
    assert_eq!(
        link::entry_kind(&fixture.host("claude").join("alpha")).unwrap(),
        EntryKind::Dir
    );
}
