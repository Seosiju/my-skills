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
         [audit]\nthreshold = 'medium'\n\
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
    fs::write(fixture.canonical("alpha").join("SKILL.md"), "Invalid.").unwrap();
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
    assert_eq!(state, fs::read(fixture.state()).unwrap());
    assert_eq!(
        fs::read_to_string(fixture.host("claude").join("alpha/SKILL.md")).unwrap(),
        "Invalid."
    );
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
fn explicit_sync_audits_fresh_links_across_hosts_without_yes() {
    let fixture = repo();
    fixture
        .command()
        .args(["install", "alpha", "--host", "codex", "--yes"])
        .assert()
        .success();
    skill(
        &fixture.registry.join("skills"),
        "alpha",
        "A synthetic test skill.",
        "![Example](https://example.invalid/remote.png)",
    );
    let state = fs::read(fixture.state()).unwrap();
    fixture
        .command()
        .args(["sync", "alpha"])
        .assert()
        .code(1)
        .stdout(contains("AUDIT BLOCKED"))
        .stdout(contains("markdown-remote-image"));
    let preview = support_h::json(
        fixture
            .command()
            .args(["sync", "alpha", "--dry-run", "--json"]),
    );
    assert_eq!(preview["blocked"], true);
    assert_eq!(preview["audit"]["blocked"], true);
    assert_eq!(preview["block_reasons"][0]["kind"], "audit");
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
        .stdout(contains("[BLOCKED] alpha: validation failed"));
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
        .code(1);
    let report: Value = serde_json::from_slice(&preview.get_output().stdout).unwrap();
    assert_eq!(report["blocked"], true);
    assert_eq!(report["applicable"], false);
    assert!(
        report["block_reasons"]
            .as_array()
            .unwrap()
            .iter()
            .any(|reason| reason["kind"] == "validation"
                && reason["message"]
                    .as_str()
                    .unwrap()
                    .contains("__pycache__/fixture.md"))
    );
    fixture
        .command()
        .args(["sync", "beta", "--host", "claude", "--yes"])
        .assert()
        .code(1)
        .stdout(contains("__pycache__/fixture.md"));
    assert!(!fixture.host("claude").join("beta").exists());
    assert_eq!(fs::read(fixture.state()).unwrap(), before);
}

#[test]
fn mixed_modes_report_validation_and_audit_once_per_skill() {
    let fixture = repo();
    fixture
        .command()
        .args(["install", "alpha", "--host", "codex", "--mode", "copy"])
        .assert()
        .success();
    let state = fs::read(fixture.state()).unwrap();
    skill(
        &fixture.registry.join("skills"),
        "alpha",
        "A fixture skill.",
        "![Remote](https://example.invalid/fixture.png)",
    );
    fixture
        .command()
        .args(["sync", "alpha", "--yes"])
        .assert()
        .code(1)
        .stdout(contains("alpha: blocked").count(1))
        .stdout(contains("markdown-remote-image").count(1));
    let preview =
        support_h::json(
            fixture
                .command()
                .args(["sync", "alpha", "--dry-run", "--json", "--yes"]),
        );
    assert_eq!(preview["audit"]["skills"].as_array().unwrap().len(), 1);
    assert_eq!(preview["block_reasons"].as_array().unwrap().len(), 1);

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
    assert_eq!(
        json["validation"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["host"].is_null())
            .count(),
        1
    );
    assert_eq!(
        json["block_reasons"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["host"].is_null())
            .count(),
        1
    );
    assert_eq!(fs::read(fixture.state()).unwrap(), state);
}

#[test]
fn convert_to_copy_preview_reports_audit_block() {
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
        .code(1)
        .stdout(contains("AUDIT WOULD BLOCK"))
        .stdout(contains("markdown-remote-image"));
    fixture
        .command()
        .args([
            "convert", "alpha", "--to", "copy", "--host", "claude", "--yes",
        ])
        .assert()
        .code(1)
        .stdout(contains("AUDIT BLOCKED"));
    assert_eq!(fs::read(fixture.state()).unwrap(), before);
}
