mod support_h;

use std::fs;

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::json;
use support_h::{Fixture, json, link_dir, plain, skill};

fn share(fixture: &Fixture) -> Command {
    let mut command = fixture.command();
    command.args(["share", "--from", "claude"]);
    command
}

fn host_skill(fixture: &Fixture, name: &str) -> std::path::PathBuf {
    skill(
        &fixture.host("claude"),
        name,
        "A host-only skill.",
        "# Example\n",
    )
}

#[test]
fn share_plan_json_lists_host_candidates_and_risks() {
    let fixture = Fixture::repo();
    let source = host_skill(&fixture, "brand");
    let bad = fixture.host("claude").join("bad");
    fs::create_dir(&bad).unwrap();
    fs::write(bad.join("SKILL.md"), "---\ndescription: [unclosed\n---\n").unwrap();
    let original = fs::read(fixture.manifest()).unwrap();
    let plan = json(share(&fixture).args(["--plan", "--json"]));
    let candidates = plan["candidates"].as_array().unwrap();
    assert_eq!(plan["from"], "claude");
    assert_eq!(plan["source"], plain(&fixture.host("claude")));
    assert_eq!(candidates.len(), 2);
    assert_eq!(candidates[0]["name"], "bad");
    assert_eq!(candidates[0]["canonical_status"], "missing");
    assert_eq!(candidates[0]["recommended"], "share-enable");
    assert_eq!(
        candidates[0]["choices"],
        json!(["share-enable", "share-disable", "skip"])
    );
    assert_eq!(candidates[0]["risks"].as_array().unwrap().len(), 1);
    assert_eq!(candidates[0]["risks"][0]["severity"], "warning");
    assert!(
        candidates[0]["risks"][0]["message"]
            .as_str()
            .unwrap()
            .contains("frontmatter")
    );
    assert_eq!(
        candidates[1],
        json!({
            "name": "brand",
            "description": "A host-only skill.",
            "source": plain(&source),
            "canonical": plain(&fixture.canonical("brand")),
            "canonical_status": "missing",
            "content_hash": my_skills::drive::hash::hash_directory(&source).unwrap(),
            "risks": [],
            "choices": ["share-enable", "share-disable", "skip"],
            "recommended": "share-enable",
        })
    );
    assert_eq!(fs::read(fixture.manifest()).unwrap(), original);
    assert!(!fixture.state().exists());
    assert!(!fixture.canonical("brand").exists());
}

#[test]
fn share_plan_table_matches_v041() {
    let fixture = Fixture::repo();
    share(&fixture)
        .arg("--plan")
        .assert()
        .success()
        .stdout(format!(
            "no skills found in {}\n",
            plain(&fixture.host("claude"))
        ));
    host_skill(&fixture, "brand");
    share(&fixture).arg("--plan").assert().success().stdout(
        "Skill  Canonical  Recommended   Risks\n-----  ---------  ------------  -----\nbrand  missing    share-enable  none \n",
    );
}

#[test]
fn share_apply_imports_registers_and_adopts_source() {
    let fixture = Fixture::repo();
    let source = host_skill(&fixture, "example");
    let contents = fs::read(source.join("SKILL.md")).unwrap();
    share(&fixture).args(["example", "--enable"]).assert().success().stderr("").stdout(format!(
        "shared: example -> {} (enabled)\nadopted: example -> claude\nNext: `my-skills sync example --host <host>`.\n",
        plain(&fixture.canonical("example"))
    ));
    let settings = fixture.config();
    let row = &settings["skills"]["example"];
    assert_eq!(row["enabled"].as_bool(), Some(true));
    assert_eq!(
        row["hosts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect::<Vec<_>>(),
        ["claude", "codex"]
    );
    assert_eq!(
        fs::read(fixture.canonical("example").join("SKILL.md")).unwrap(),
        contents
    );
    assert_eq!(fs::read(source.join("SKILL.md")).unwrap(), contents);
    assert!(!fixture.host("codex").exists());
    let state = my_skills::drive::state::State::load(Some(&fixture.state())).unwrap();
    let record = state.get("example", "claude").unwrap();
    assert_eq!(record.source_type, "host");
    assert_eq!(
        record.mode,
        my_skills::registry::manifest::InstallMode::Copy
    );
    assert_eq!(record.destination, plain(&source));
    assert_eq!(record.installed_hash, record.source_hash);
    assert_eq!(record.source_url, plain(&source));
    assert_eq!(record.source_revision, record.source_hash);

    let catalog = json(
        fixture
            .command()
            .args(["skills", "--with-status", "--json"]),
    );
    let row = &catalog["skills"][0];
    assert_eq!(
        row["status"],
        json!({"claude": "FRESH", "codex": "MISSING"})
    );
    assert_eq!(row["provenance"]["trust_tier"], "imported-local");
    assert_eq!(
        row["provenance"]["last_audit_result_hash"],
        record.last_audit_result_hash
    );
}

#[test]
fn share_apply_imports_registers_adopts_source_and_syncs() {
    let fixture = Fixture::repo();
    host_skill(&fixture, "example");
    share(&fixture)
        .args(["example", "--enable"])
        .assert()
        .success();
    fixture
        .command()
        .args(["sync", "example"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("--yes"));
    assert!(!fixture.host("codex").join("example").exists());
    fixture
        .command()
        .args(["sync", "example", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("unchanged: example -> claude"));
    assert!(fixture.host("codex").join("example/SKILL.md").is_file());
}

#[test]
fn share_apply_blocks_different_canonical_without_force() {
    let fixture = Fixture::repo();
    host_skill(&fixture, "brand");
    let canonical = skill(
        &fixture.registry.join("skills"),
        "brand",
        "Existing canonical.",
        "# Existing\n",
    );
    let original = fs::read(canonical.join("SKILL.md")).unwrap();
    share(&fixture).args(["brand", "--enable"]).assert().code(1).stdout(format!(
        "[BLOCKED] brand: a different canonical skill already exists at {}; re-run with --force to overwrite it\n",
        plain(&canonical)
    ));
    assert_eq!(fs::read(canonical.join("SKILL.md")).unwrap(), original);
    share(&fixture)
        .args(["brand", "--enable", "--force"])
        .assert()
        .success()
        .stdout(predicate::str::starts_with(
            "WARN: force overwrite allowed for existing canonical skill\n",
        ));
    assert!(
        fs::read_to_string(canonical.join("SKILL.md"))
            .unwrap()
            .contains("# Example")
    );
}

#[test]
fn share_apply_disable_registers_disabled_skill() {
    let fixture = Fixture::repo();
    host_skill(&fixture, "brand");
    share(&fixture)
        .args(["brand", "--disable"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Next: `my-skills enable brand`, then `my-skills sync brand --host <host>`.",
        ));
    assert_eq!(
        fixture.config()["skills"]["brand"]["enabled"].as_bool(),
        Some(false)
    );
    assert!(fixture.canonical("brand").join("SKILL.md").is_file());
}

#[test]
fn share_apply_records_manifest_audit_policy() {
    let fixture = Fixture::repo();
    fixture.write_manifest("\n[audit]\nprofile = \"permissive\"\n");
    host_skill(&fixture, "brand");
    share(&fixture)
        .args(["brand", "--enable"])
        .assert()
        .success();
    let state = my_skills::drive::state::State::load(Some(&fixture.state())).unwrap();
    let record = state.get("brand", "claude").unwrap();
    assert_eq!(record.audit_profile, "permissive");
    assert_eq!(record.audit_threshold, "none");
}

#[test]
fn share_rejects_links_in_apply_and_plan() {
    let fixture = Fixture::repo();
    let brand = host_skill(&fixture, "brand");
    let elsewhere = skill(
        &fixture.root.join("elsewhere"),
        "linked",
        "Outside.",
        "# Outside\n",
    );
    link_dir(&elsewhere, &fixture.host("claude").join("linked"));
    link_dir(&elsewhere, &brand.join("refs"));
    let before = fs::read(fixture.manifest()).unwrap();
    for (name, link) in [("brand", "refs"), ("linked", ".")] {
        share(&fixture)
            .args([name, "--enable", "--skip-audit"])
            .assert()
            .code(1)
            .stdout(format!(
                "[BLOCKED] {name}: share source contains symlink\n  symlink: {link}\n"
            ));
    }
    let plan = json(share(&fixture).args(["--plan", "--json"]));
    let candidates = plan["candidates"].as_array().unwrap();
    assert_eq!(candidates.len(), 2);
    for candidate in candidates {
        assert_eq!(candidate["choices"], json!(["skip"]));
        assert_eq!(candidate["risks"][0]["severity"], "error");
        assert!(
            candidate["risks"][0]["message"]
                .as_str()
                .unwrap()
                .contains("symlink")
        );
    }
    assert_eq!(fs::read(fixture.manifest()).unwrap(), before);
    assert!(!fixture.state().exists());
    assert!(!fixture.canonical("brand").exists());
    assert!(!fixture.canonical("linked").exists());
}

#[test]
fn share_enforces_audit_threshold_and_explicit_skip() {
    let fixture = Fixture::repo();
    fixture.write_manifest("\n[audit]\nthreshold = \"low\"\n");
    skill(
        &fixture.host("claude"),
        "brand",
        "A host-only skill.",
        "# Example\n\n![Badge](https://example.invalid/badge.png)\n",
    );
    let plan = json(share(&fixture).args(["--plan", "--json"]));
    assert_eq!(plan["candidates"][0]["recommended"], "skip");
    share(&fixture)
        .args(["brand", "--enable"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains(
            "[BLOCKED] brand: validation failed",
        ));
    share(&fixture)
        .args(["brand", "--enable", "--skip-audit"])
        .assert()
        .success()
        .stdout(predicate::str::starts_with(
            "WARN: audit skipped by explicit --skip-audit\n",
        ));
}

#[test]
fn share_rolls_back_canonical_and_manifest_when_state_save_fails() {
    let fixture = Fixture::repo();
    let local = host_skill(&fixture, "brand");
    let canonical = skill(
        &fixture.registry.join("skills"),
        "brand",
        "Old canonical.",
        "# Old\n",
    );
    let manifest = fs::read(fixture.manifest()).unwrap();
    let before = fs::read(canonical.join("SKILL.md")).unwrap();
    let host_before = fs::read(local.join("SKILL.md")).unwrap();
    // The state module exclusively creates this staging file, so a collision
    // fails after publication and exercises both rollback steps.
    let state = fixture.state();
    let collision = state.with_file_name("state.json.tmp");
    fs::create_dir_all(state.parent().unwrap()).unwrap();
    fs::write(&collision, "occupied\n").unwrap();

    share(&fixture)
        .args(["brand", "--enable", "--force"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("state.json.tmp"));
    assert_eq!(fs::read(fixture.manifest()).unwrap(), manifest);
    assert_eq!(fs::read(canonical.join("SKILL.md")).unwrap(), before);
    assert_eq!(fs::read(local.join("SKILL.md")).unwrap(), host_before);
    assert_eq!(fs::read_to_string(collision).unwrap(), "occupied\n");
    assert!(!state.exists());
    assert_eq!(
        fs::read_dir(fixture.registry.join("skills"))
            .unwrap()
            .count(),
        1
    );
}

#[test]
fn share_requires_skill_and_enable_or_disable_for_apply() {
    let fixture = Fixture::repo();
    share(&fixture)
        .assert()
        .code(2)
        .stderr("error: share requires a skill name unless --plan is used\n");
    share(&fixture)
        .arg("brand")
        .assert()
        .code(2)
        .stderr("error: share requires exactly one of --enable or --disable\n");
    share(&fixture)
        .args(["brand", "--enable", "--disable"])
        .assert()
        .code(2);
    fixture
        .command()
        .args(["share", "--from", "ghost", "--plan"])
        .assert()
        .code(2)
        .stderr("error: unknown host: ghost\n");
}

#[test]
fn share_force_replaces_canonical_link_without_touching_its_target() {
    let fixture = Fixture::repo();
    host_skill(&fixture, "brand");
    let outside = skill(
        &fixture.root.join("elsewhere"),
        "brand",
        "Outside.",
        "# Keep\n",
    );
    let before = fs::read(outside.join("SKILL.md")).unwrap();
    let canonical = fixture.canonical("brand");
    link_dir(&outside, &canonical);
    share(&fixture)
        .args(["brand", "--enable", "--force"])
        .assert()
        .success();
    assert_eq!(
        my_skills::drive::link::entry_kind(&canonical).unwrap(),
        my_skills::drive::link::EntryKind::Dir
    );
    assert_eq!(fs::read(outside.join("SKILL.md")).unwrap(), before);
    assert!(
        fs::read_to_string(canonical.join("SKILL.md"))
            .unwrap()
            .contains("# Example")
    );
}
