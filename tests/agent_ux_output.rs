#[allow(dead_code)]
mod support_g;

use std::fs;
use std::process::Command;

use assert_cmd::{assert::Assert, prelude::*};
use my_skills::platform::paths;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use serde_json::Value;
use support_g::{Fixture, MANIFEST};

fn registry(two_hosts: bool) -> Fixture {
    let fixture = Fixture::isolated();
    let mut manifest = format!("{MANIFEST}\n[defaults]\ninstall_mode = \"copy\"\n");
    for (host, enabled) in [("claude", two_hosts), ("codex", true), ("agy", false)] {
        let path = fixture.base.join("hosts").join(host);
        manifest.push_str(&format!(
            "\n[targets.{host}]\nenabled = {enabled}\npath = '{}'\n",
            path.display()
        ));
    }
    manifest.push_str("\n[skills.example]\nenabled = true\n");
    fixture.write("my-skills.toml", &manifest);
    write_skill(&fixture, "A valid example skill.", "Example body.");
    fixture
}

fn write_skill(fixture: &Fixture, description: &str, body: &str) {
    fixture.write(
        "skills/example/SKILL.md",
        &format!("---\nname: example\ndescription: {description}\n---\n\n{body}\n"),
    );
}

fn dry_run(fixture: &Fixture, command: &str) -> Command {
    let mut cmd = fixture.command();
    cmd.args([command, "--dry-run"]);
    cmd
}

fn stdout(result: &Assert) -> &str {
    std::str::from_utf8(&result.get_output().stdout).unwrap()
}

fn json(result: &Assert) -> Value {
    serde_json::from_str(stdout(result)).unwrap()
}

#[test]
fn skills_json_emits_utf8_descriptions() {
    let fixture = registry(false);
    let description = "한국어 학습 예제 🚀";
    write_skill(&fixture, description, "Example body.");
    let out = fixture
        .command()
        .args(["skills", "--json"])
        .assert()
        .success();
    assert!(stdout(&out).contains(description));
    assert!(!stdout(&out).contains(r"\ud55c"));
    assert!(!stdout(&out).contains(r"\ud83d"));
    assert_eq!(json(&out)["skills"][0]["description"], description);
}

#[cfg(windows)]
#[test]
fn windows_displays_registry_doctor_and_plan_paths_consistently() {
    fn check_path(value: &Value) {
        let path = value.as_str().unwrap();
        assert!(
            !path.contains(r"\\?\") && !path.contains('/') && path.contains('\\'),
            "{path}"
        );
    }

    let fixture = registry(false);
    fixture.init();
    fixture.git(&["config", "core.hooksPath", "custom/hooks"]);

    let report = json(
        &fixture
            .command()
            .args(["doctor", "--no-update-check", "--json"])
            .assert()
            .success(),
    );
    for value in [
        &report["registry"]["path"],
        &report["state_path"],
        &report["data_root"],
        &report["hooks"]["path"],
    ] {
        check_path(value);
    }
    assert!(
        report["hooks"]["path"]
            .as_str()
            .unwrap()
            .ends_with(r"\custom\hooks")
    );
    for host in report["hosts"].as_array().unwrap() {
        check_path(&host["path"]);
    }

    for command in ["install", "sync"] {
        let value = json(&dry_run(&fixture, command).arg("--json").assert().success());
        check_path(&value["registry"]["path"]);
        let action = &value["actions"][0];
        check_path(&action["source"]);
        check_path(&action["destination"]);
    }

    let active = fixture.base.join("active");
    fs::create_dir(&active).unwrap();
    fs::write(active.join("my-skills.toml"), MANIFEST).unwrap();
    fixture.cache(&active);
    let state = fixture.base.join("xdg-state/my-skills/state.json");
    fs::create_dir_all(state.parent().unwrap()).unwrap();
    fs::write(&state, "{invalid").unwrap();

    for args in [
        &["doctor", "--no-update-check"][..],
        &["doctor", "--no-update-check", "--json"][..],
    ] {
        let out = fixture
            .command()
            .env_remove("MY_SKILLS_ROOT")
            .args(args)
            .assert()
            .success();
        if args.contains(&"--json") {
            let report = json(&out);
            let warning = report["registry"]["warning"].as_str().unwrap();
            assert!(warning.contains(&paths::display_path(&active)), "{warning}");
            let warning = report["connections"]["warnings"][0].as_str().unwrap();
            assert!(warning.contains(&paths::display_path(&state)), "{warning}");
        } else {
            let text = stdout(&out);
            assert!(!text.contains(r"\\?\"), "{text}");
            assert!(text.contains(&paths::display_path(&active)), "{text}");
            assert!(text.contains(&paths::display_path(&state)), "{text}");
        }
    }
}

#[test]
fn invalid_skill_fails_both_dry_runs_with_machine_and_human_diagnostics() {
    let fixture = registry(false);
    fs::remove_file(fixture.registry.join("skills/example/SKILL.md")).unwrap();
    for command in ["install", "sync"] {
        let out = dry_run(&fixture, command)
            .assert()
            .code(1)
            .stdout(contains("[BLOCKED] example: validation failed"))
            .stdout(contains("SKILL.md not found"));
        assert_eq!(
            stdout(&out).lines().next().unwrap(),
            "BLOCKED: 1 item(s) would be refused (validation 1, plan 0)"
        );

        let out = dry_run(&fixture, command)
            .arg("--json")
            .assert()
            .code(1)
            .stderr(contains("SKILL.md not found"))
            .stderr(contains("No files were changed"));
        let value = json(&out);
        assert_eq!(value["dry_run"], true);
        assert_eq!(value["blocked"], true);
        assert_eq!(value["applicable"], false);
        let validation = value["validation"].as_array().unwrap();
        let check = validation
            .iter()
            .find(|check| check["host"].is_null())
            .unwrap();
        assert_eq!(check["skill"], "example");
        assert_eq!(check["ok"], false);
        assert_eq!(check["errors"], serde_json::json!(["SKILL.md not found"]));
        assert_eq!(check["warnings"], serde_json::json!([]));
        assert_eq!(value["block_reasons"][0]["kind"], "validation");
        assert!(value["block_reasons"][0]["host"].is_null());
    }
    assert!(!fixture.base.join("hosts").exists());
}

#[test]
fn unmanaged_destination_is_reported_by_previews_and_enforced_by_real_runs() {
    let fixture = registry(false);
    let destination = fixture.base.join("hosts/codex/example");
    fs::create_dir_all(&destination).unwrap();
    let original = destination.join("SKILL.md");
    fs::write(&original, "Local skill.").unwrap();
    for command in ["install", "sync"] {
        let out = dry_run(&fixture, command).assert().success();
        assert_eq!(
            stdout(&out).lines().next().unwrap(),
            "BLOCKED: 1 item(s) would be refused (validation 0, plan 1)"
        );
        let value = json(&dry_run(&fixture, command).arg("--json").assert().success());
        assert_eq!(value["blocked"], true);
        assert_eq!(value["applicable"], false);
        assert_eq!(value["block_reasons"][0]["kind"], "block_conflict");
        assert_eq!(value["block_reasons"][0]["skill"], "example");
        assert_eq!(value["block_reasons"][0]["host"], "codex");
        fixture
            .command()
            .arg(command)
            .assert()
            .code(1)
            .stdout(contains("BLOCKED"));
        assert_eq!(fs::read_to_string(&original).unwrap(), "Local skill.");
    }
    assert!(!fixture.base.join("xdg-state/my-skills/state.json").exists());
}

#[test]
fn drifted_copy_is_reported_without_turning_preview_into_an_exit_code_gate() {
    let fixture = registry(false);
    fixture.command().arg("sync").assert().success();
    let copy = fixture.base.join("hosts/codex/example/SKILL.md");
    fs::write(&copy, "Locally changed.").unwrap();

    let out = dry_run(&fixture, "sync").assert().success();
    assert_eq!(
        stdout(&out).lines().next().unwrap(),
        "BLOCKED: 1 item(s) would be refused (validation 0, plan 1)"
    );
    let value = json(&dry_run(&fixture, "sync").arg("--json").assert().success());
    assert_eq!(value["blocked"], true);
    assert_eq!(value["applicable"], false);
    assert_eq!(value["actions"][0]["action"], "BLOCK_DRIFT");
    assert_eq!(value["block_reasons"][0]["kind"], "block_drift");
    assert_eq!(value["block_reasons"][0]["host"], "codex");
    fixture
        .command()
        .arg("sync")
        .assert()
        .code(1)
        .stdout(contains("BLOCKED: example -> codex"));
    assert_eq!(fs::read_to_string(copy).unwrap(), "Locally changed.");
}

#[test]
fn clean_previews_show_whether_multi_host_confirmation_is_needed() {
    let fixture = registry(true);
    for command in ["install", "sync"] {
        let value = json(&dry_run(&fixture, command).arg("--json").assert().success());
        assert_eq!(value["blocked"], false);
        assert_eq!(value["applicable"], false);
        assert_eq!(value["block_reasons"][0]["kind"], "needs_yes");
        assert_eq!(value["registry"]["source"], "env");
        assert_eq!(
            value["registry"]["path"],
            paths::display_path(&fixture.registry)
        );
        assert_eq!(value["actions"].as_array().unwrap().len(), 2);
        fixture
            .command()
            .arg(command)
            .assert()
            .code(2)
            .stderr(contains("writes to multiple hosts"));

        let value = json(
            &dry_run(&fixture, command)
                .args(["--json", "--yes"])
                .assert()
                .success(),
        );
        assert_eq!(value["blocked"], false);
        assert_eq!(value["applicable"], true);
        assert!(value.get("audit").is_none());
        assert!(value["block_reasons"].as_array().unwrap().is_empty());
    }
    assert!(!fixture.base.join("hosts").exists());

    fixture.command().args(["sync", "--yes"]).assert().success();
    let value = json(&dry_run(&fixture, "sync").arg("--json").assert().success());
    assert_eq!(value["applicable"], true, "no writes need confirmation");
}

#[test]
fn sync_json_applies_by_default() {
    let fixture = registry(false);
    let out = fixture
        .command()
        .args(["sync", "--json"])
        .assert()
        .success();
    assert_eq!(json(&out)["actions"][0]["result"], "created");
    assert!(fixture.base.join("hosts/codex/example/SKILL.md").is_file());
}

#[test]
fn sync_check_does_not_cache_a_registry_discovered_from_cwd() {
    let fixture = registry(false);
    fixture
        .command()
        .env_remove("MY_SKILLS_ROOT")
        .args(["sync", "--check"])
        .assert()
        .code(1)
        .stdout(contains("MISSING"));
    assert!(!fixture.base.join("xdg-config").exists());
}

fn with_different_active_registry() -> Fixture {
    let fixture = registry(false);
    let active = fixture.base.join("active");
    fs::create_dir(&active).unwrap();
    fs::write(active.join("my-skills.toml"), MANIFEST).unwrap();
    fixture.cache(&active);
    fixture
}

#[test]
fn inactive_registry_blocks_install_and_sync_json_plans() {
    let fixture = with_different_active_registry();
    for command in ["install", "sync"] {
        let real = fixture
            .command()
            .env_remove("MY_SKILLS_ROOT")
            .args([command, "--yes"])
            .assert()
            .code(2)
            .stderr(contains("refusing to write to cwd registry"));
        let stderr = std::str::from_utf8(&real.get_output().stderr).unwrap();
        let message = stderr.trim().strip_prefix("error: ").unwrap();
        let out = dry_run(&fixture, command)
            .env_remove("MY_SKILLS_ROOT")
            .args(["--json", "--yes"])
            .assert()
            .success();
        let plan = json(&out);
        assert_eq!(plan["blocked"], true, "{command}: {plan}");
        assert_eq!(plan["applicable"], false);
        assert_eq!(
            plan["block_reasons"],
            serde_json::json!([{
                "skill": null,
                "host": null,
                "kind": "inactive_registry",
                "message": message,
            }])
        );
        assert_eq!(plan["actions"][0]["action"], "CREATE");
    }
    assert!(!fixture.base.join("hosts").exists());
}

#[test]
fn inactive_registry_refusal_starts_each_text_plan() {
    let fixture = with_different_active_registry();
    let mut first_lines = Vec::new();
    for args in [
        &["install", "--dry-run"][..],
        &["sync", "--dry-run"][..],
        &["uninstall", "example", "--dry-run"][..],
        &["convert", "--to", "link", "--dry-run"][..],
    ] {
        let out = fixture
            .command()
            .env_remove("MY_SKILLS_ROOT")
            .args(args)
            .assert()
            .success()
            .stdout(contains("planned actions"));
        first_lines.push(stdout(&out).lines().next().unwrap().to_owned());
    }
    assert!(
        first_lines
            .iter()
            .all(|line| line.starts_with("BLOCKED: refusing to write to cwd registry ")),
        "{first_lines:?}"
    );

    fs::remove_file(fixture.registry.join("skills/example/SKILL.md")).unwrap();
    for command in ["install", "sync"] {
        let out = dry_run(&fixture, command)
            .env_remove("MY_SKILLS_ROOT")
            .assert()
            .code(1);
        let mut lines = stdout(&out).lines();
        assert_eq!(lines.next().unwrap(), first_lines[0]);
        assert_eq!(
            lines.next().unwrap(),
            "BLOCKED: 1 item(s) would be refused (validation 1, plan 0)"
        );
    }
}

#[test]
fn sync_check_wins_over_dry_run() {
    let fixture = registry(false);
    let check = fixture.command().args(["sync", "--check"]).assert().code(1);
    let combined = fixture
        .command()
        .args(["sync", "--check", "--dry-run"])
        .assert()
        .code(1);
    assert_eq!(stdout(&combined), stdout(&check));
    assert!(stdout(&combined).contains("MISSING"));
}

#[test]
fn sync_json_conflicts_with_check_even_with_dry_run() {
    let fixture = registry(false);
    for extra in [&["--dry-run"][..], &[][..]] {
        fixture
            .command()
            .args(["sync", "--check", "--json"])
            .args(extra)
            .assert()
            .code(2)
            .stderr(contains(
                "sync --json is not supported with --check; use --dry-run --json",
            ));
    }
}

#[test]
fn connect_usage_errors_leave_empty_config_home_alone() {
    let mut cached = Vec::new();
    for (args, error) in [
        (&["install", "absent", "--json"][..], "unknown skill"),
        (&["uninstall"][..], "uninstall requires a skill name"),
        (
            &["sync", "--check", "--json"][..],
            "not supported with --check",
        ),
        (&["install", "absent"][..], "unknown skill"),
        (&["sync", "absent"][..], "unknown skill"),
        (&["convert", "absent", "--to", "link"][..], "unknown skill"),
        (
            &["uninstall", "example", "--host", "absent"][..],
            "unknown host",
        ),
        (&["bootstrap", "--host", "absent"][..], "unknown host"),
    ] {
        let fixture = registry(false);
        fixture
            .command()
            .env_remove("MY_SKILLS_ROOT")
            .args(args)
            .assert()
            .code(2)
            .stderr(contains(error));
        if fixture.base.join("xdg-config").exists() {
            cached.push(args);
        }
    }
    assert!(
        cached.is_empty(),
        "usage errors created a cache: {cached:?}"
    );
}

#[test]
fn multi_host_confirmation_is_the_last_line_of_text_plans() {
    let fixture = registry(true);
    let expected =
        "Re-run with --yes to apply: this command writes to multiple hosts (claude, codex).";
    let mut last_lines = Vec::new();
    for args in [
        &["install", "--dry-run"][..],
        &["sync", "--dry-run"][..],
        &["uninstall", "example", "--dry-run"][..],
        &["convert", "--to", "link", "--dry-run"][..],
    ] {
        let out = fixture
            .command()
            .env_remove("MY_SKILLS_ROOT")
            .args(args)
            .assert()
            .success()
            .stdout(contains("as the active registry"));
        assert!(!stdout(&out).starts_with("BLOCKED:"));
        last_lines.push(stdout(&out).lines().last().unwrap().to_owned());
        fixture
            .command()
            .args(args)
            .arg("--yes")
            .assert()
            .success()
            .stdout(contains(expected).not());
    }
    assert_eq!(last_lines, [expected; 4]);

    fixture.command().args(["sync", "--yes"]).assert().success();
    let noop = dry_run(&fixture, "sync").assert().success();
    assert!(!stdout(&noop).contains(expected));
}
