#[allow(dead_code)]
mod support_g;

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::{assert::Assert, prelude::*};
use my_skills::drive::link::{self, Created};
use my_skills::platform::paths::display_path;
use predicates::str::contains;
use serde_json::{Value, json};
use support_g::{Fixture, MANIFEST, skill};

fn fixture() -> Fixture {
    let fixture = Fixture::isolated();
    let mut manifest = format!(
        "{MANIFEST}\n[defaults]\ninstall_mode = \"link\"\n\n[audit]\nthreshold = \"medium\"\n"
    );
    for (host, enabled, mode) in [
        ("agy", false, "copy"),
        ("claude", true, "link"),
        ("codex", true, "copy"),
    ] {
        let path = fixture.base.join("hosts").join(host);
        manifest.push_str(&format!(
            "\n[targets.{host}]\nenabled = {enabled}\npath = '{}'\ninstall_mode = '{mode}'\n",
            display_path(&path)
        ));
    }
    manifest
        .push_str("\n[skills.example]\ngroup = 'checks'\n\n[skills.other]\nhosts = ['claude']\n");
    fixture.write("my-skills.toml", &manifest);
    fixture.write("skills/example/SKILL.md", &skill("example"));
    fixture.write("skills/other/SKILL.md", &skill("other"));
    fixture
}

fn read_json(assert: &Assert) -> Value {
    serde_json::from_slice(&assert.get_output().stdout).unwrap()
}

fn dest(fixture: &Fixture, host: &str, name: &str) -> PathBuf {
    fixture.base.join("hosts").join(host).join(name)
}

fn make_link(path: &Path, target: &Path) {
    assert!(matches!(
        link::create_link(path, target).unwrap(),
        Created::Link(_)
    ));
}

fn state_path(fixture: &Fixture) -> PathBuf {
    fixture.base.join("xdg-state/my-skills/state.json")
}

fn set_record(fixture: &Fixture, host: &str, name: &str, extra: Value) {
    let path = state_path(fixture);
    let mut state: Value = if path.exists() {
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap()
    } else {
        json!({"schema_version": 1, "installs": []})
    };
    let mut record = json!({
        "skill": name, "host": host, "mode": "copy",
        "source": display_path(&fixture.registry.join("skills").join(name)),
        "destination": display_path(&dest(fixture, host, name)),
        "source_hash": "", "installed_hash": "", "installed_at": ""
    });
    record
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    let installs = state["installs"].as_array_mut().unwrap();
    installs.retain(|r| r["host"] != host || r["skill"] != name);
    installs.push(record);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, serde_json::to_vec(&state).unwrap()).unwrap();
}

fn host<'a>(report: &'a Value, field: &str, value: &str) -> &'a Value {
    report["hosts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row[field] == value)
        .unwrap()
}

#[test]
fn validate_shares_the_gate_and_respects_host_and_group_selection() {
    let fixture = fixture();
    let long = format!(
        "---\nname: example\ndescription: {}\nallowed-tools: Read\n---\n\n# Example\n",
        "x".repeat(800)
    );
    fixture.write("skills/example/SKILL.md", &long);

    fixture
        .command()
        .args(["validate", "example"])
        .assert()
        .code(1)
        .stdout(contains("[FAIL] example"))
        .stdout(contains("codex description exceeds 768 characters"));

    let selected = read_json(
        &fixture
            .command()
            .args([
                "validate", "other", "example", "--group", "checks", "--json",
            ])
            .assert()
            .code(1),
    );
    assert_eq!(
        selected["registry"],
        json!({"path": display_path(&fixture.registry), "source": "env"})
    );
    assert_eq!(selected["ok"], false);
    let rows = selected["skills"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["skill"], "example");
    assert_eq!(rows[0]["ok"], false);
    assert_eq!(
        rows[0]["errors"],
        json!([
            "claude description exceeds 768 characters",
            "codex description exceeds 768 characters"
        ])
    );
    assert!(
        rows[0]["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|warning| {
                warning
                    .as_str()
                    .unwrap()
                    .contains("inconsistent host support")
            })
    );
    assert_eq!(
        rows[1],
        json!({"skill": "other", "ok": true, "errors": [], "warnings": []})
    );
    let narrowed = read_json(
        &fixture
            .command()
            .args(["validate", "example", "--host", "agy", "--json"])
            .assert()
            .code(1),
    );
    assert_eq!(
        narrowed["skills"][0]["errors"],
        json!(["agy description exceeds 768 characters"])
    );

    let doctor = read_json(
        &fixture
            .command()
            .args(["doctor", "--no-update-check", "--json"])
            .assert()
            .success(),
    );
    assert_eq!(
        doctor["full_validation"]["failures"][0]["errors"],
        selected["skills"][0]["errors"]
    );
    fixture.write(
        "skills/example/SKILL.md",
        &format!(
            "{}\n![Example](https://example.invalid/pixel.png)",
            skill("example")
        ),
    );
    let audit = read_json(
        &fixture
            .command()
            .args(["validate", "example", "--json"])
            .assert()
            .code(1),
    );
    assert!(
        audit["skills"][0]["errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|error| { error.as_str().unwrap().starts_with("audit (") })
    );
}

#[test]
fn validate_reports_promoted_finding_once_and_keeps_other_warnings() {
    let fixture = fixture();
    let markdown = concat!(
        "---\nname: example\ndescription: A valid example skill.\n",
        "allowed-tools: Read\n---\n\n",
        "![Example](https://example.invalid/pixel.png)\n",
    );
    fixture.write("skills/example/SKILL.md", markdown);
    let check = |code| {
        read_json(
            &fixture
                .command()
                .args(["validate", "example", "--json"])
                .assert()
                .code(code),
        )
    };
    let promoted = check(1);
    let row = &promoted["skills"][0];
    assert_eq!(row["errors"].as_array().unwrap().len(), 1);
    assert!(
        row["errors"][0]
            .as_str()
            .unwrap()
            .contains("markdown-remote-image")
    );
    let warnings = row["warnings"].as_array().unwrap();
    assert!(
        warnings
            .iter()
            .any(|w| w.as_str().unwrap().contains("allowed-tools"))
    );
    assert!(
        !warnings
            .iter()
            .any(|w| w.as_str().unwrap().contains("example.invalid/pixel.png"))
    );

    fixture.write("my-skills.local.toml", "[audit]\nthreshold = 'high'\n");
    let unpromoted = check(0);
    assert!(
        unpromoted["skills"][0]["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| { w.as_str().unwrap().contains("example.invalid/pixel.png") })
    );
}

#[test]
fn validate_default_includes_unregistered_skill_directories() {
    let fixture = fixture();
    fixture.write("skills/extra/notes.txt", "Missing skill metadata.");
    let value = read_json(
        &fixture
            .command()
            .env_remove("MY_SKILLS_ROOT")
            .args(["validate", "--json"])
            .assert()
            .code(1),
    );
    assert_eq!(value["registry"]["source"], "cwd");
    assert_eq!(
        value["skills"][1],
        json!({
            "skill": "extra", "ok": false, "errors": ["SKILL.md not found"], "warnings": []
        })
    );
    assert!(!fixture.base.join("xdg-config").exists());
}

fn assert_error(assert: &Assert, code: &str, text: &str) {
    let value = read_json(assert);
    let error = &value["error"];
    assert_eq!(value.as_object().unwrap().len(), 1);
    assert_eq!(error.as_object().unwrap().len(), 3);
    assert_eq!(error["code"], code);
    assert!(error["message"].as_str().unwrap().contains(text), "{value}");
    assert!(error["hint"].is_null() || error["hint"].is_string());
}

#[test]
fn json_errors_include_usage_selection_and_config_codes() {
    let fixture = fixture();
    for (args, code, message) in [
        (
            &["sync", "--check", "--json"][..],
            "usage",
            "not supported with --check",
        ),
        (
            &["share", "--from", "codex", "--json"][..],
            "usage",
            "requires a skill name",
        ),
        (
            &["audit", "absent", "--json"][..],
            "unknown_skill",
            "unknown skill",
        ),
        (
            &["validate", "--group", "absent", "--json"][..],
            "unknown_group",
            "unknown group",
        ),
        (
            &["validate", "--host", "absent", "--json"][..],
            "unknown_host",
            "unknown host",
        ),
    ] {
        let assert = fixture
            .command()
            .args(args)
            .assert()
            .code(2)
            .stderr(contains("error:"))
            .stderr(contains(message));
        assert_error(&assert, code, message);
    }
    let assert = fixture
        .command()
        .env("MY_SKILLS_ROOT", fixture.base.join("outside"))
        .args(["validate", "--json"])
        .assert()
        .code(2)
        .stderr(contains("error:"));
    assert_error(&assert, "config", "does not contain");

    fixture.write("my-skills.local.toml", "[audit]\nthreshold = 'invalid'\n");
    let assert = fixture
        .command()
        .args(["validate", "--json"])
        .assert()
        .code(2);
    assert_error(&assert, "config", "invalid");
}

#[test]
fn json_write_refusal_and_blocked_preview_keep_their_own_shapes() {
    let fixture = fixture();
    let active = fixture.base.join("outside");
    fs::write(active.join("my-skills.toml"), MANIFEST).unwrap();
    fixture.cache(&active);

    let assert = fixture
        .command()
        .env_remove("MY_SKILLS_ROOT")
        .args(["share", "example", "--from", "codex", "--enable", "--json"])
        .assert()
        .code(2)
        .stderr(contains("refusing to write to cwd registry"));
    assert_error(&assert, "inactive_registry", "refusing to write");

    let value = read_json(
        &fixture
            .command()
            .env_remove("MY_SKILLS_ROOT")
            .args(["sync", "--dry-run", "--json"])
            .assert()
            .success(),
    );
    assert_eq!(value["block_reasons"][0]["kind"], "inactive_registry");
    assert!(value.get("error").is_none());
}

#[test]
fn clap_usage_errors_keep_stderr_and_emit_json_only_when_asked() {
    let fixture = fixture();
    for (args, message) in [
        (&["validate", "--absent", "--json"][..], "--absent"),
        (&["validate", "--json", "--host"][..], "--host"),
        (&["diff", "--json"][..], "<SKILL>"),
    ] {
        let assert = fixture
            .command()
            .args(args)
            .assert()
            .code(2)
            .stderr(contains("error:"))
            .stderr(contains(message));
        assert_error(&assert, "usage", message);
    }
    fixture
        .command()
        .args(["validate", "--absent"])
        .assert()
        .code(2)
        .stdout("");
    fixture
        .command()
        .args(["validate", "--json", "--help"])
        .assert()
        .success()
        .stdout(contains("Usage:"));
    fixture
        .command()
        .args(["--version", "--json"])
        .assert()
        .success()
        .stdout(format!("my-skills {}\n", env!("CARGO_PKG_VERSION")));
}

#[test]
fn doctor_quick_keeps_keys_and_counts_only_owned_connections_per_host() {
    let fixture = fixture();
    fixture.init();
    set_record(&fixture, "claude", "example", json!({"mode": "link"}));
    set_record(
        &fixture,
        "codex",
        "example",
        json!({"fallback_reason": "example link unavailable"}),
    );
    set_record(&fixture, "codex", "other", json!({}));
    set_record(
        &fixture,
        "codex",
        "foreign",
        json!({"source": display_path(&fixture.base.join("outside/skills/foreign"))}),
    );
    let report = read_json(
        &fixture
            .command()
            .args(["doctor", "--quick", "--json"])
            .assert()
            .success(),
    );
    let claude = host(&report, "name", "claude");
    let codex = host(&report, "name", "codex");
    assert_eq!(claude["configured_mode"], "link");
    assert_eq!(
        claude["connections"],
        json!({"link": 1, "copy": 0, "fallback": 0})
    );
    assert_eq!(codex["configured_mode"], "copy");
    assert_eq!(
        codex["connections"],
        json!({"link": 0, "copy": 2, "fallback": 1})
    );

    for key in ["registry_git", "hooks", "hooks_error", "full_validation"] {
        assert_eq!(report.get(key), Some(&Value::Null), "{key}");
    }
    assert_eq!(report["update"], "not_checked");
    assert_eq!(
        report["link_capability"],
        json!({"kind": "not_checked", "reason": null})
    );
    assert_eq!(report["manifest"]["status"], "valid");

    let full = read_json(
        &fixture
            .command()
            .env("MY_SKILLS_NO_UPDATE_CHECK", "1")
            .args(["doctor", "--json"])
            .assert()
            .success(),
    );
    assert_eq!(full["update"], "skipped");
    assert_eq!(full["registry_git"]["branch"], "main");
    assert_eq!(full["full_validation"]["checked"], 2);
    assert_ne!(full["link_capability"]["kind"], "not_checked");
    fixture
        .command()
        .args(["doctor", "--quick"])
        .assert()
        .success()
        .stdout(contains("mode=copy"))
        .stdout(contains("link 0 / copy 2 (fallback 1)"));

    fixture.write("my-skills.toml", "bad = [");
    let invalid = read_json(
        &fixture
            .command()
            .args(["doctor", "--quick", "--json"])
            .assert()
            .code(1),
    );
    assert_eq!(invalid["manifest"]["status"], "invalid");
    assert_eq!(
        host(&invalid, "name", "codex")["configured_mode"],
        Value::Null
    );
}

fn compare(fixture: &Fixture, args: &[&str], exit: i32) -> Value {
    let assert = fixture
        .command()
        .env_remove("MY_SKILLS_ROOT")
        .args(["diff", "example", "--json"])
        .args(args)
        .assert()
        .code(exit);
    let result = read_json(&assert);
    assert_eq!(result["skill"], "example");
    assert_eq!(
        result["registry"],
        json!({"path": display_path(&fixture.registry), "source": "cwd"})
    );
    assert_eq!(result["different"], exit == 1);
    assert!(!fixture.base.join("xdg-config").exists());
    result
}

#[test]
fn diff_copy_files_agree_with_fresh_stale_drifted_and_conflict_status() {
    for (source_changed, installed_changed, expected) in [
        (false, false, "FRESH"),
        (true, false, "STALE"),
        (false, true, "DRIFTED"),
        (true, true, "CONFLICT"),
    ] {
        let fixture = fixture();
        fixture.write("skills/example/sub/changed.txt", "base\n");
        fixture.write("skills/example/removed.txt", "base\n");
        fixture
            .command()
            .args(["install", "example", "--host", "codex"])
            .assert()
            .success();
        let installed = dest(&fixture, "codex", "example");
        fs::create_dir(installed.join(".omc")).unwrap();
        fs::write(installed.join(".omc/cache"), "ignored").unwrap();
        fixture.write("skills/example/.DS_Store", "ignored");
        if source_changed {
            fixture.write("skills/example/removed.txt", "canonical\n");
            fixture.write("skills/example/new.txt", "canonical only\n");
        }
        if installed_changed {
            fs::write(installed.join("sub/changed.txt"), "local\n").unwrap();
            fs::remove_file(installed.join("removed.txt")).unwrap();
            fs::write(installed.join("added.txt"), "installed only\n").unwrap();
        }
        let state_before = fs::read(state_path(&fixture)).unwrap();
        let compared = compare(
            &fixture,
            &["--host", "codex"],
            i32::from(source_changed || installed_changed),
        );
        assert_eq!(compared["hosts"].as_array().unwrap().len(), 1);
        let row = host(&compared, "host", "codex");
        let status = read_json(
            &fixture
                .command()
                .args(["status", "--json"])
                .assert()
                .success(),
        );
        let status = status
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["skill"] == "example" && r["host"] == "codex")
            .unwrap();
        assert_eq!(row["status"], status["status"]);
        assert_eq!(row["status"], expected);
        assert_eq!(row["mode"], "copy");
        assert_eq!(row["destination"], display_path(&installed));
        assert_eq!(row["owner"], Value::Null);
        assert_eq!(row["patch"], Value::Null);
        let files = match expected {
            "FRESH" => json!({"added": [], "removed": [], "modified": []}),
            "STALE" => json!({"added": [], "removed": ["new.txt"], "modified": ["removed.txt"]}),
            "DRIFTED" => {
                json!({"added": ["added.txt"], "removed": ["removed.txt"], "modified": ["sub/changed.txt"]})
            }
            _ => {
                json!({"added": ["added.txt"], "removed": ["new.txt", "removed.txt"], "modified": ["sub/changed.txt"]})
            }
        };
        assert_eq!(row["files"], files);

        if expected == "CONFLICT" {
            let source = fixture.registry.join("skills/example");
            for name in ["new.txt", "removed.txt", "sub/changed.txt"] {
                fs::copy(source.join(name), installed.join(name)).unwrap();
            }
            fs::remove_file(installed.join("added.txt")).unwrap();
            let converged = compare(&fixture, &["--host", "codex"], 0);
            let row = host(&converged, "host", "codex");
            assert_eq!(row["status"], "CONFLICT");
            assert_eq!(
                row["files"],
                json!({"added": [], "removed": [], "modified": []})
            );
        }
        assert_eq!(fs::read(state_path(&fixture)).unwrap(), state_before);
    }
}

#[test]
fn diff_patch_reports_text_binary_and_large_files() {
    let fixture = fixture();
    let installed = dest(&fixture, "codex", "example");
    fs::create_dir_all(&installed).unwrap();
    fs::copy(
        fixture.registry.join("skills/example/SKILL.md"),
        installed.join("SKILL.md"),
    )
    .unwrap();
    fixture.write("skills/example/modified.txt", "before\n");
    fixture.write("skills/example/removed.txt", "canonical only\n");
    fs::write(installed.join("modified.txt"), "after\n").unwrap();
    fs::write(installed.join("added.txt"), "host only\n").unwrap();
    fs::write(installed.join("binary.dat"), b"\x00\xff").unwrap();
    fs::write(installed.join("large.txt"), vec![b'x'; 1024 * 1024 + 1]).unwrap();
    let value = compare(&fixture, &["--host", "codex", "--patch"], 1);
    let row = host(&value, "host", "codex");
    assert_eq!(row["status"], "UNMANAGED");
    let patch = row["patch"].as_str().unwrap();
    for part in [
        "--- a/modified.txt",
        "+++ b/modified.txt",
        "-before",
        "+after",
        "/dev/null",
        "+host only",
        "-canonical only",
        "binary.dat",
        "binary",
        "large.txt",
        "1 MiB",
    ] {
        assert!(patch.contains(part), "{part}: {patch}");
    }
    assert!(!state_path(&fixture).exists());
}

#[test]
fn diff_reports_the_actual_link_target_in_text_and_json() {
    let fixture = fixture();
    let installed = dest(&fixture, "claude", "example");
    let target = fixture.base.join("outside/redirected");
    fs::create_dir_all(installed.parent().unwrap()).unwrap();
    fs::create_dir_all(&target).unwrap();
    make_link(&installed, &target);
    set_record(&fixture, "claude", "example", json!({"mode": "link"}));

    let report = compare(&fixture, &["--host", "claude"], 1);
    let row = host(&report, "host", "claude");
    assert_eq!(row["status"], "DRIFTED");
    assert_eq!(row["link_matches"], false);
    assert_eq!(row["link_target"], display_path(&target));
    fixture
        .command()
        .args(["diff", "example", "--host", "claude"])
        .assert()
        .code(1)
        .stdout(contains(display_path(&target)));
}

#[test]
fn diff_explicit_disabled_host_checks_managed_copy() {
    let fixture = fixture();
    fixture
        .command()
        .args(["install", "example", "--host", "agy"])
        .assert()
        .success();
    fs::write(dest(&fixture, "agy", "example").join("SKILL.md"), "local\n").unwrap();

    let report = compare(&fixture, &["--host", "agy"], 1);
    let row = host(&report, "host", "agy");
    assert_eq!(row["status"], "DRIFTED");
    assert_eq!(row["files"]["modified"], json!(["SKILL.md"]));
    let default = compare(&fixture, &[], 1);
    assert!(
        default["hosts"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["host"] != "agy")
    );
}

#[test]
fn diff_explicit_host_reports_unmanaged_and_missing_even_when_excluded() {
    let fixture = fixture();
    fixture.write(
        "my-skills.local.toml",
        "[skills.example]\nhosts = ['claude']\n",
    );
    let installed = dest(&fixture, "agy", "example");
    fs::create_dir_all(&installed).unwrap();
    fs::write(installed.join("local.txt"), "host only\n").unwrap();

    let unmanaged = compare(&fixture, &["--host", "agy"], 1);
    let row = host(&unmanaged, "host", "agy");
    assert_eq!(row["status"], "UNMANAGED");
    assert_eq!(row["files"]["added"], json!(["local.txt"]));
    assert_eq!(row.get("link_target"), Some(&Value::Null));

    let missing = compare(&fixture, &["--host", "codex"], 1);
    let row = host(&missing, "host", "codex");
    assert_eq!(row["status"], "MISSING");
    assert_eq!(
        row["destination"],
        display_path(&dest(&fixture, "codex", "example"))
    );
    assert!(!state_path(&fixture).exists());
}

#[test]
fn diff_equal_foreign_copy_is_still_different() {
    let fixture = fixture();
    let installed = dest(&fixture, "codex", "example");
    fs::create_dir_all(&installed).unwrap();
    fs::copy(
        fixture.registry.join("skills/example/SKILL.md"),
        installed.join("SKILL.md"),
    )
    .unwrap();
    let foreign = fixture.base.join("outside/skills");
    set_record(
        &fixture,
        "codex",
        "example",
        json!({"source": display_path(&foreign.join("example"))}),
    );

    let report = compare(&fixture, &["--host", "codex"], 1);
    let row = host(&report, "host", "codex");
    assert_eq!(row["status"], "FOREIGN");
    assert_eq!(row["owner"], display_path(&foreign));
    assert_eq!(
        row["files"],
        json!({"added": [], "removed": [], "modified": []})
    );
}

#[test]
fn diff_uses_foreign_records_actual_destination() {
    let fixture = fixture();
    let installed = fixture.base.join("outside/custom/install");
    fs::create_dir_all(&installed).unwrap();
    fs::copy(
        fixture.registry.join("skills/example/SKILL.md"),
        installed.join("SKILL.md"),
    )
    .unwrap();
    fs::write(installed.join("local.txt"), "host only\n").unwrap();
    let foreign = fixture.base.join("outside/skills");
    set_record(
        &fixture,
        "codex",
        "example",
        json!({
            "source": display_path(&foreign.join("example")),
            "destination": display_path(&installed),
        }),
    );
    let before = fs::read(state_path(&fixture)).unwrap();

    let report = compare(&fixture, &["--host", "codex", "--patch"], 1);
    let row = host(&report, "host", "codex");
    assert_eq!(row["status"], "FOREIGN");
    assert_eq!(row["owner"], display_path(&foreign));
    assert_eq!(row["destination"], display_path(&installed));
    assert_eq!(row["files"]["added"], json!(["local.txt"]));
    assert!(row["patch"].as_str().unwrap().contains("+host only"));
    assert_eq!(fs::read(state_path(&fixture)).unwrap(), before);
    assert!(!dest(&fixture, "codex", "example").exists());
}

#[test]
fn diff_uses_owned_records_when_the_configured_path_changes() {
    let fixture = fixture();
    fixture
        .command()
        .args(["install", "example", "--host", "codex"])
        .assert()
        .success();
    let installed = dest(&fixture, "codex", "example");
    let changed_path = fixture.base.join("outside/new-target");
    fixture.write(
        "my-skills.local.toml",
        &format!(
            "[targets.codex]\npath = '{}'\n",
            display_path(&changed_path)
        ),
    );

    let fresh = compare(&fixture, &["--host", "codex"], 0);
    let row = host(&fresh, "host", "codex");
    assert_eq!(row["status"], "FRESH");
    assert_eq!(row["destination"], display_path(&installed));

    fs::write(installed.join("SKILL.md"), "edited copy\n").unwrap();
    let drifted = compare(&fixture, &["--host", "codex"], 1);
    let row = host(&drifted, "host", "codex");
    assert_eq!(row["status"], "DRIFTED");
    assert_eq!(row["files"]["modified"], json!(["SKILL.md"]));
}

#[test]
fn diff_foreign_link_to_canonical_is_still_different() {
    let fixture = fixture();
    fixture.write(
        "my-skills.local.toml",
        "[skills.example]\nhosts = ['codex']\n",
    );
    let installed = dest(&fixture, "claude", "example");
    let canonical = fixture.registry.join("skills/example");
    fs::create_dir_all(installed.parent().unwrap()).unwrap();
    make_link(&installed, &canonical);
    set_record(
        &fixture,
        "claude",
        "example",
        json!({
            "mode": "link",
            "source": display_path(&fixture.base.join("outside/skills/example")),
        }),
    );

    let report = compare(&fixture, &["--host", "claude"], 1);
    let row = host(&report, "host", "claude");
    assert_eq!(row["status"], "FOREIGN");
    assert_eq!(row["link_matches"], true);
    assert_eq!(row["link_target"], display_path(&canonical));
}

#[test]
fn diff_reports_missing_foreign_and_link_targets_without_writing() {
    let fixture = fixture();
    let missing = compare(&fixture, &[], 1);
    assert_eq!(missing["hosts"].as_array().unwrap().len(), 2);
    assert_eq!(host(&missing, "host", "codex")["status"], "MISSING");
    assert!(!state_path(&fixture).exists());

    let canonical = fixture.registry.join("skills/example");
    let installed = dest(&fixture, "claude", "example");
    fs::create_dir_all(installed.parent().unwrap()).unwrap();
    make_link(&installed, &canonical);
    set_record(&fixture, "claude", "example", json!({"mode": "link"}));
    let linked = compare(&fixture, &["--host", "claude", "--patch"], 0);
    let row = host(&linked, "host", "claude");
    assert_eq!(row["status"], "FRESH");
    assert_eq!(row["mode"], "link");
    assert_eq!(row["link_matches"], true);
    assert_eq!(
        row["files"],
        json!({"added": [], "removed": [], "modified": []})
    );
    assert_eq!(row["patch"], Value::Null);

    let foreign = fixture.base.join("outside/skills");
    let foreign_skill = foreign.join("example");
    fs::create_dir_all(&foreign_skill).unwrap();
    fs::write(foreign_skill.join("unrelated.txt"), "foreign\n").unwrap();
    link::remove_link(&installed).unwrap();
    make_link(&installed, &foreign_skill);
    let drifted = compare(&fixture, &["--host", "claude"], 1);
    assert_eq!(host(&drifted, "host", "claude")["status"], "DRIFTED");
    assert_eq!(host(&drifted, "host", "claude")["link_matches"], false);

    set_record(
        &fixture,
        "claude",
        "example",
        json!({"mode": "link", "source": display_path(&foreign_skill)}),
    );
    let foreign_report = compare(&fixture, &["--host", "claude", "--patch"], 1);
    let row = host(&foreign_report, "host", "claude");
    assert_eq!(row["status"], "FOREIGN");
    assert_eq!(row["owner"], display_path(&foreign));
    assert_eq!(
        row["files"],
        json!({"added": [], "removed": [], "modified": []})
    );
    assert_eq!(row["patch"], Value::Null);

    link::remove_link(&installed).unwrap();
    make_link(&installed, &canonical);
    set_record(&fixture, "claude", "example", json!({"mode": "link"}));
    fixture.write(
        "my-skills.local.toml",
        "[skills.example]\nenabled = false\n",
    );
    let retained = compare(&fixture, &[], 0);
    assert_eq!(retained["hosts"].as_array().unwrap().len(), 1);
    assert_eq!(retained["hosts"][0]["status"], "FRESH");

    fs::rename(&canonical, fixture.registry.join("skills/moved")).unwrap();
    let broken = compare(&fixture, &[], 1);
    assert_eq!(broken["hosts"][0]["status"], "BROKEN");
    assert_eq!(broken["hosts"][0]["link_matches"], true);

    fixture
        .command()
        .args(["diff", "example", "--host", "claude"])
        .assert()
        .code(1)
        .stdout(contains("BROKEN"))
        .stdout(contains("link points to the canonical skill"));

    for (args, code, message) in [
        (
            &["diff", "absent", "--json"][..],
            "unknown_skill",
            "unknown skill",
        ),
        (
            &["diff", "example", "--host", "absent", "--json"][..],
            "unknown_host",
            "unknown host",
        ),
    ] {
        let assert = fixture.command().args(args).assert().code(2);
        assert_error(&assert, code, message);
    }
}
