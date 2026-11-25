mod support_h;

use std::fs::{self, OpenOptions};

use fs4::FileExt;
use my_skills::drive::{link, state::State};
use my_skills::platform::paths::display_path;
use serde_json::{Value, json};
use support_h::{Fixture, skill};

fn registry() -> Fixture {
    let fixture = Fixture::repo();
    fixture.write_manifest(
        "[audit]\nthreshold = 'medium'\n\
         [skills.alpha]\n[skills.beta]\n[skills.retired]\n",
    );
    let manifest = fixture.manifest();
    let text = fs::read_to_string(&manifest).unwrap().replace(
        "[targets.codex]\n",
        "[targets.codex]\ninstall_mode = 'copy'\n",
    );
    fs::write(manifest, text).unwrap();
    for name in ["alpha", "beta", "retired"] {
        skill(
            &fixture.registry.join("skills"),
            name,
            "An isolated fixture skill.",
            "Test body.",
        );
    }
    fixture
}

fn result(fixture: &Fixture, args: &[&str], expected_code: i32) -> Value {
    let result = fixture
        .command()
        .args(args)
        .arg("--json")
        .assert()
        .code(expected_code);
    // from_slice also rejects any second document or human lines.
    serde_json::from_slice(&result.get_output().stdout)
        .unwrap_or_else(|err| panic!("{args:?}: {err}: {:?}", result.get_output()))
}

fn row<'a>(report: &'a Value, skill: &str, host: &str) -> &'a Value {
    report["actions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["skill"] == skill && row["host"] == host)
        .unwrap_or_else(|| panic!("missing {skill}/{host}: {report}"))
}

fn assert_summary(
    report: &Value,
    [created, updated, removed, unchanged, blocked, failed]: [u32; 6],
) {
    assert_eq!(
        report["summary"],
        json!({
            "created": created, "updated": updated, "removed": removed,
            "unchanged": unchanged, "blocked": blocked, "failed": failed,
        })
    );
}

#[test]
fn install_and_sync_report_creates_updates_prunes_and_noops() {
    let fixture = registry();
    let install = result(&fixture, &["install", "alpha", "retired", "--yes"], 0);
    assert_eq!(install["dry_run"], false);
    assert_eq!(install["applied"], true);
    assert_eq!(install["applicable"], true);
    assert_eq!(install["blocked"], false);
    assert_eq!(install["block_reasons"], json!([]));
    assert_eq!(install["cleanup_warnings"], json!([]));
    assert_eq!(install["registry"]["source"], "env");
    assert_summary(&install, [4, 0, 0, 0, 0, 0]);
    for (host, mode) in [("claude", "link"), ("codex", "copy")] {
        let item = row(&install, "alpha", host);
        assert_eq!(item["result"], "created");
        assert_eq!(item["action"], "CREATE");
        assert_eq!(item["requested_mode"], mode);
        assert_eq!(item["mode"], mode);
        assert!(item["error"].is_null());
        assert!(item["fallback_reason"].is_null());
    }
    assert!(matches!(
        link::entry_kind(&fixture.host("claude").join("alpha")).unwrap(),
        link::EntryKind::Link(_)
    ));
    assert_eq!(
        link::entry_kind(&fixture.host("codex").join("alpha")).unwrap(),
        link::EntryKind::Dir
    );

    fs::write(fixture.canonical("alpha").join("note.txt"), "An update.").unwrap();
    fixture
        .command()
        .args(["disable", "retired"])
        .assert()
        .success();
    let sync = result(&fixture, &["sync", "--yes"], 0);
    assert_eq!(sync["applied"], true);
    assert_summary(&sync, [2, 1, 2, 1, 0, 0]);
    assert_eq!(row(&sync, "alpha", "claude")["result"], "unchanged");
    assert_eq!(row(&sync, "alpha", "codex")["result"], "updated");
    assert_eq!(row(&sync, "beta", "claude")["result"], "created");
    assert_eq!(row(&sync, "retired", "claude")["result"], "removed");
    assert_eq!(
        fs::read_to_string(fixture.host("codex").join("alpha/note.txt")).unwrap(),
        "An update."
    );
    assert!(
        State::load(Some(&fixture.state()))
            .unwrap()
            .get("retired", "claude")
            .is_none()
    );
    assert!(!fixture.host("codex").join("retired").exists());

    let before = fs::read(fixture.state()).unwrap();
    let noop = result(&fixture, &["sync"], 0);
    assert_eq!(noop["applied"], true);
    assert_eq!(noop["applicable"], true);
    assert_summary(&noop, [0, 0, 0, 4, 0, 0]);
    assert!(
        noop["actions"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["result"] == "unchanged")
    );
    assert_eq!(fs::read(fixture.state()).unwrap(), before);
}

#[test]
fn validation_audit_and_confirmation_blocks_return_the_batch_document() {
    let fixture = registry();
    for command in ["install", "sync"] {
        fs::remove_file(fixture.canonical("alpha").join("SKILL.md")).unwrap();
        let invalid = result(&fixture, &[command, "alpha", "beta", "--yes"], 1);
        assert_eq!(invalid["applied"], false);
        assert_eq!(invalid["blocked"], true);
        assert_eq!(invalid["applicable"], false);
        assert_summary(&invalid, [0, 0, 0, 0, 4, 0]);
        assert!(
            invalid["block_reasons"]
                .as_array()
                .unwrap()
                .iter()
                .any(|reason| reason["kind"] == "validation")
        );
        assert_eq!(row(&invalid, "beta", "claude")["result"], "blocked");

        skill(
            &fixture.registry.join("skills"),
            "alpha",
            "A fixture skill.",
            "![Remote](https://example.invalid/fixture.png)",
        );
        let audit = result(&fixture, &[command, "alpha", "--yes"], 1);
        assert_eq!(audit["applied"], false);
        assert_eq!(audit["audit"]["blocked"], true);
        assert_eq!(audit["block_reasons"][0]["kind"], "audit");
        assert_summary(&audit, [0, 0, 0, 0, 2, 0]);

        skill(
            &fixture.registry.join("skills"),
            "alpha",
            "A fixture skill.",
            "Test body.",
        );
        let needs_yes = result(&fixture, &[command, "alpha"], 2);
        assert_eq!(needs_yes["applied"], false);
        assert_eq!(needs_yes["blocked"], false);
        assert_eq!(needs_yes["applicable"], false);
        assert_eq!(needs_yes["block_reasons"][0]["kind"], "needs_yes");
        assert_summary(&needs_yes, [0, 0, 0, 0, 2, 0]);
        assert!(!fixture.root.join("hosts").exists());
        assert!(!fixture.state().exists());
        assert!(!fixture.root.join("config/my-skills/root").exists());
    }
}

#[test]
fn inactive_registry_and_shared_destination_block_the_batch() {
    let fixture = registry();
    let active = registry();
    fixture
        .command()
        .args(["set-root", active.registry.to_str().unwrap()])
        .assert()
        .success();
    let cache = fixture.root.join("config/my-skills/root");
    let before = fs::read(&cache).unwrap();
    for command in ["install", "sync"] {
        let blocked = fixture
            .command()
            .env_remove("MY_SKILLS_ROOT")
            .args([command, "alpha", "--json", "--yes"])
            .assert()
            .code(2);
        let report: Value = serde_json::from_slice(&blocked.get_output().stdout).unwrap();
        assert_eq!(report["applied"], false);
        assert_eq!(report["blocked"], true);
        assert_eq!(report["block_reasons"][0]["kind"], "inactive_registry");
        assert_summary(&report, [0, 0, 0, 0, 2, 0]);
        assert_eq!(fs::read(&cache).unwrap(), before);
        assert!(!fixture.state().exists());
    }

    let manifest = fixture.manifest();
    let contents = fs::read_to_string(&manifest).unwrap().replace(
        &serde_json::to_string(&fixture.host("codex")).unwrap(),
        &serde_json::to_string(&fixture.host("claude")).unwrap(),
    );
    fs::write(manifest, contents).unwrap();
    let batch = result(&fixture, &["install", "alpha", "--yes"], 1);
    assert_eq!(batch["applied"], false);
    assert_eq!(batch["blocked"], true);
    assert!(!fixture.root.join("hosts").exists());
    assert_summary(&batch, [0, 0, 0, 0, 2, 0]);
}

#[test]
fn per_item_blocks_and_save_failures_do_not_claim_success() {
    let fixture = registry();
    let host = fixture.host("codex");
    fs::create_dir_all(host.join("alpha")).unwrap();
    let partially_blocked = result(
        &fixture,
        &["install", "alpha", "beta", "--host", "codex"],
        1,
    );
    assert_eq!(partially_blocked["applied"], false);
    assert_eq!(
        row(&partially_blocked, "alpha", "codex")["result"],
        "blocked"
    );
    assert_eq!(
        row(&partially_blocked, "beta", "codex")["result"],
        "created"
    );
    assert_summary(&partially_blocked, [1, 0, 0, 0, 1, 0]);

    let state = fs::read(fixture.state()).unwrap();
    fs::write(
        fixture.state().with_file_name("state.json.tmp"),
        "Reserved.",
    )
    .unwrap();
    let failed = result(&fixture, &["install", "retired", "--host", "codex"], 1);
    assert_eq!(failed["applied"], false);
    assert_eq!(failed["applicable"], true);
    let row = row(&failed, "retired", "codex");
    assert_eq!(row["result"], "failed");
    assert!(row["error"].as_str().unwrap().contains("state.json.tmp"));
    assert_summary(&failed, [0, 0, 0, 0, 0, 1]);
    assert_eq!(fs::read(fixture.state()).unwrap(), state);
    assert!(!host.join("retired").exists());
}

#[test]
fn configuration_errors_use_the_existing_error_document() {
    let fixture = registry();
    for command in ["install", "sync"] {
        let unknown = result(&fixture, &[command, "absent"], 2);
        assert_eq!(unknown.as_object().unwrap().len(), 1);
        assert_eq!(unknown["error"]["code"], "unknown_skill");
    }
    let path = fixture.state().with_file_name("state.json.lock");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let lock = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .unwrap();
    FileExt::lock(&lock).unwrap();
    for command in ["install", "sync"] {
        let busy = fixture
            .command()
            .args([command, "--json", "--yes"])
            .env("MY_SKILLS_STATE_LOCK_TIMEOUT_MS", "0")
            .assert()
            .code(2);
        let value: Value = serde_json::from_slice(&busy.get_output().stdout).unwrap();
        assert_eq!(value.as_object().unwrap().len(), 1);
        assert_eq!(value["error"]["code"], "state_busy");
    }
    assert!(!fixture.state().exists());
    assert!(!fixture.root.join("hosts").exists());
}

#[test]
fn dry_run_json_keeps_the_wave15_bytes() {
    let fixture = registry();
    let canonical = display_path(&fixture.canonical("alpha"));
    let expected = json!({
        "actions": [{
            "action": "CREATE", "destination": display_path(&fixture.host("codex").join("alpha")),
            "host": "codex", "mode": "copy", "reason": "destination missing",
            "skill": "alpha", "source": canonical,
        }],
        "applicable": true,
        "audit": {
            "blocked": false,
            "skills": [{
                "blocked": false, "findings": [], "path": canonical, "profile": "default",
                "result_hash": "sha256:4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945",
                "skill": "alpha", "threshold": "medium",
            }],
            "skipped": false,
        },
        "blocked": false, "block_reasons": [],
        "dry_run": true, "registry": {"path": display_path(&fixture.registry), "source": "env"},
        "validation": [
            {"errors": [], "host": null, "ok": true, "skill": "alpha", "warnings": []},
            {"errors": [], "host": "codex", "ok": true, "skill": "alpha", "warnings": []},
        ],
    });
    let expected = format!("{}\n", serde_json::to_string_pretty(&expected).unwrap());
    for command in ["install", "sync"] {
        let out = fixture
            .command()
            .args([command, "alpha", "--host", "codex", "--dry-run", "--json"])
            .assert()
            .success()
            .stderr("");
        assert_eq!(out.get_output().stdout, expected.as_bytes());
    }
    assert!(!fixture.state().exists());
    assert!(!fixture.root.join("hosts").exists());
}

#[cfg(windows)]
#[test]
fn committed_cleanup_warning_has_the_backup_path_in_json() {
    let fixture = registry();
    fixture
        .command()
        .args(["install", "alpha", "--host", "codex"])
        .assert()
        .success();
    let installed = fixture.host("codex").join("alpha");
    let relative = "assets/note.txt";
    let protected = installed.join(relative);
    let source = fixture.canonical("alpha").join(relative);
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(&source, "Initial fixture.").unwrap();
    fixture
        .command()
        .args(["sync", "alpha", "--host", "codex"])
        .assert()
        .success();
    fs::write(&source, "Updated fixture.").unwrap();
    let who = fixture
        .process("whoami")
        .args(["/user", "/fo", "csv", "/nh"])
        .output()
        .unwrap();
    assert!(who.status.success());
    let csv = String::from_utf8(who.stdout).unwrap();
    let account = format!(
        "*{}",
        csv.trim().rsplit_once(',').unwrap().1.trim_matches('"')
    );
    let acl = |path: &std::path::Path, action: &str, rule: &str| {
        assert!(
            fixture
                .process("icacls")
                .arg(path)
                .args([action, rule])
                .output()
                .unwrap()
                .status
                .success()
        );
    };
    acl(&protected, "/deny", &format!("{account}:(DE)"));
    acl(
        protected.parent().unwrap(),
        "/deny",
        &format!("{account}:(DC)"),
    );

    let out = fixture
        .command()
        .args(["sync", "alpha", "--host", "codex", "--json"])
        .output()
        .unwrap();
    let backup = fs::read_dir(fixture.host("codex"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(".alpha.backup-")
        });
    let protected = backup
        .as_ref()
        .map_or(protected, |path| path.join(relative));
    if protected.is_file() {
        acl(&protected, "/remove:d", &account);
        acl(protected.parent().unwrap(), "/remove:d", &account);
    }

    assert!(out.status.success(), "{out:?}");
    let backup = backup.expect("committed update retained its protected backup");
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["applied"], true);
    assert_summary(&report, [0, 1, 0, 0, 0, 0]);
    let warnings = report["cleanup_warnings"].as_array().unwrap();
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0]["skill"], "alpha");
    assert_eq!(warnings[0]["host"], "codex");
    assert_eq!(warnings[0]["path"], display_path(&backup));
    assert!(warnings[0]["message"].as_str().unwrap().contains("cleanup"));
    assert_eq!(
        fs::read_to_string(installed.join(relative)).unwrap(),
        "Updated fixture."
    );
    assert_eq!(fs::read_to_string(protected).unwrap(), "Initial fixture.");
}

#[cfg(windows)]
#[test]
fn a_target_too_long_for_a_windows_link_reports_the_copy_fallback() {
    let fixture = registry();
    // NTFS can read this tree, but the target cannot fit in the 16 KiB
    // reparse data buffer used to create a junction or directory symlink.
    let segment = "fixture".repeat(16);
    let skills_root = (0..80).fold(fixture.registry.clone(), |path, _| path.join(&segment));
    let root = serde_json::to_string(&skills_root).unwrap();
    let manifest = fixture.manifest();
    let text = fs::read_to_string(&manifest)
        .unwrap()
        .replace("skills_root = \"skills\"", &format!("skills_root = {root}"));
    fs::write(manifest, text).unwrap();
    skill(&skills_root, "alpha", "A fixture skill.", "Test body.");

    let applied = result(&fixture, &["install", "alpha", "--host", "claude"], 0);
    assert_eq!(applied["applied"], true);
    assert_summary(&applied, [1, 0, 0, 0, 0, 0]);
    let row = row(&applied, "alpha", "claude");
    assert_eq!(row["requested_mode"], "link");
    assert_eq!(row["mode"], "copy");
    assert_eq!(row["result"], "created");
    let reason = row["fallback_reason"].as_str().unwrap();
    assert!(reason.contains("junction failed"), "{reason}");
    let state = State::load(Some(&fixture.state())).unwrap();
    assert_eq!(
        state
            .get("alpha", "claude")
            .unwrap()
            .fallback_reason
            .as_deref(),
        Some(reason)
    );
    assert_eq!(
        link::entry_kind(&fixture.host("claude").join("alpha")).unwrap(),
        link::EntryKind::Dir
    );
}
