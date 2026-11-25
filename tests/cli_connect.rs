//! Black-box tests for the integrator's commands: install, sync, uninstall,
//! convert, bootstrap, validate, audit, status, set-root, data-path.

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use my_skills::drive::link::{self, EntryKind};
use my_skills::drive::state::State;
use my_skills::platform::paths;
use my_skills::registry::manifest::InstallMode;
use predicates::prelude::*;
use predicates::str::contains;

/// Links are junctions on Windows and symlinks elsewhere (spec D1).
const LINK_KIND: &str = if cfg!(windows) { "junction" } else { "symlink" };

struct Env {
    tmp: tempfile::TempDir,
}

impl Env {
    fn new() -> Self {
        let env = Env {
            tmp: tempfile::tempdir().unwrap(),
        };
        for skill in ["alpha", "beta"] {
            env.write_skill(skill, &format!("Demo skill {skill}."));
        }
        env.write_manifest(true);
        env
    }

    fn root(&self) -> PathBuf {
        self.tmp.path().join("registry")
    }

    fn host(&self, name: &str) -> PathBuf {
        self.tmp.path().join("hosts").join(name)
    }

    fn state_path(&self) -> PathBuf {
        self.tmp.path().join("state/my-skills/state.json")
    }

    fn state(&self) -> State {
        State::load(Some(&self.state_path())).unwrap()
    }

    fn block_state_save(&self) -> PathBuf {
        let blocker = self.state_path().with_file_name("state.json.tmp");
        fs::create_dir_all(blocker.parent().unwrap()).unwrap();
        fs::write(&blocker, "busy").unwrap();
        blocker
    }

    fn write_skill(&self, name: &str, description: &str) {
        let dir = self.root().join("skills").join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: {description}\n---\n\nBody.\n"),
        )
        .unwrap();
    }

    fn write_manifest(&self, alpha_enabled: bool) {
        let mut toml = String::from("schema_version = 1\nskills_root = \"skills\"\n");
        for (host, enabled) in [
            ("claude", true),
            ("codex", true),
            ("agy", true),
            ("custom", false),
        ] {
            toml.push_str(&format!(
                "\n[targets.{host}]\nenabled = {enabled}\npath = '{}'\n",
                self.host(host).display()
            ));
        }
        toml.push_str(&format!("\n[skills.alpha]\nenabled = {alpha_enabled}\n"));
        toml.push_str("\n[skills.beta]\nenabled = true\nhosts = [\"claude\"]\n");
        fs::write(self.root().join("my-skills.toml"), toml).unwrap();
    }

    fn cmd(&self) -> Command {
        let mut cmd = Command::cargo_bin("my-skills").unwrap();
        let base = self.tmp.path();
        cmd.current_dir(base)
            .env("HOME", base.join("home"))
            .env("USERPROFILE", base.join("home"))
            .env("LOCALAPPDATA", base.join("local-app-data"))
            .env("APPDATA", base.join("appdata"))
            .env("XDG_STATE_HOME", base.join("state"))
            .env("XDG_DATA_HOME", base.join("data"))
            .env("XDG_CONFIG_HOME", base.join("config"))
            .env("MY_SKILLS_ROOT", self.root());
        cmd
    }
}

fn is_link(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}

#[test]
fn install_dry_run_json_plans_links_by_default() {
    let env = Env::new();
    let out = env
        .cmd()
        .args(["install", "--dry-run", "--json"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let actions = value["actions"].as_array().unwrap();
    // alpha -> claude, codex, agy; beta -> claude (beta's hosts list).
    let creates: Vec<_> = actions.iter().filter(|a| a["action"] == "CREATE").collect();
    assert_eq!(creates.len(), 4, "{value}");
    assert!(creates.iter().all(|a| a["mode"] == "link"));
    assert_eq!(value["audit"]["skipped"], false);
    assert!(
        !env.host("claude").join("alpha").exists(),
        "dry run wrote files"
    );
}

#[test]
fn install_json_applies_by_default() {
    let env = Env::new();
    let assert = env
        .cmd()
        .args(["install", "alpha", "--host", "claude", "--json"])
        .assert()
        .success();
    let report: serde_json::Value = serde_json::from_slice(&assert.get_output().stdout).unwrap();
    assert_eq!(report["applied"], true);
    assert_eq!(report["actions"][0]["result"], "created");
    assert!(env.host("claude").join("alpha/SKILL.md").is_file());
}

#[test]
fn multi_host_write_needs_yes() {
    let env = Env::new();
    env.cmd()
        .arg("sync")
        .assert()
        .code(2)
        .stderr(contains("writes to multiple hosts"));
    assert!(!env.host("claude").join("alpha").exists());
}

#[test]
fn sync_links_then_status_is_fresh_and_check_passes() {
    let env = Env::new();
    env.cmd()
        .args(["sync", "--yes"])
        .assert()
        .success()
        .stdout(contains(format!(
            "created: alpha -> claude (link: {LINK_KIND})"
        )));
    assert!(is_link(&env.host("codex").join("alpha")));
    assert!(
        !env.host("codex").join("beta").exists(),
        "beta is claude-only"
    );

    env.cmd()
        .arg("status")
        .assert()
        .success()
        .stdout(contains("claude   FRESH"));
    env.cmd().args(["sync", "--check"]).assert().success();

    let out = env.cmd().args(["status", "--json"]).output().unwrap();
    let rows: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let alpha_claude = rows
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["skill"] == "alpha" && r["host"] == "claude")
        .unwrap();
    assert_eq!(alpha_claude["status"], "FRESH");
    assert_eq!(alpha_claude["mode"], "link");
    assert_eq!(alpha_claude["link_kind"], LINK_KIND);
}

#[test]
fn disabling_then_syncing_prunes_links() {
    let env = Env::new();
    env.cmd().args(["sync", "--yes"]).assert().success();
    env.write_manifest(false);
    env.cmd()
        .args(["sync", "--yes"])
        .assert()
        .success()
        .stdout(contains("removed: alpha -> claude"));
    for host in ["claude", "codex", "agy"] {
        assert!(!env.host(host).join("alpha").exists(), "{host} kept alpha");
    }
    assert!(env.root().join("skills/alpha/SKILL.md").is_file(), "I1");
    assert!(is_link(&env.host("claude").join("beta")));
}

#[test]
fn sync_quiet_prints_nothing_when_up_to_date() {
    let env = Env::new();
    env.cmd().args(["sync", "--yes"]).assert().success();
    env.cmd()
        .args(["sync", "--yes", "--quiet"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}

#[test]
fn uninstall_removes_one_host_link_only() {
    let env = Env::new();
    env.cmd().args(["sync", "--yes"]).assert().success();
    env.cmd()
        .args(["uninstall", "alpha", "--host", "claude"])
        .assert()
        .success()
        .stdout(contains("removed: alpha -> claude"));
    assert!(!env.host("claude").join("alpha").exists());
    assert!(is_link(&env.host("codex").join("alpha")));
    assert!(env.root().join("skills/alpha/SKILL.md").is_file(), "I1");
}

#[test]
fn uninstall_requires_a_skill() {
    let env = Env::new();
    env.cmd()
        .arg("uninstall")
        .assert()
        .code(2)
        .stderr(contains("uninstall requires a skill name"));
}

#[test]
fn copy_install_then_convert_to_link() {
    let env = Env::new();
    env.cmd()
        .args(["install", "--mode", "copy", "--yes"])
        .assert()
        .success()
        .stdout(contains("created: alpha -> claude"));
    let copy = env.host("claude").join("alpha");
    assert!(copy.is_dir() && !is_link(&copy));

    env.cmd()
        .args(["convert", "--to", "link", "--dry-run"])
        .assert()
        .success()
        .stdout(contains("CONVERT"));
    env.cmd()
        .args(["convert", "--to", "link", "--yes"])
        .assert()
        .success()
        .stdout(contains(format!(
            "converted: alpha -> claude (link: {LINK_KIND})"
        )));
    assert!(is_link(&copy));
    env.cmd()
        .arg("status")
        .assert()
        .stdout(contains("claude   FRESH"));
}

#[test]
fn validate_reports_ok_and_fail() {
    let env = Env::new();
    env.cmd()
        .arg("validate")
        .assert()
        .success()
        .stdout(contains("[OK] alpha").and(contains("[OK] beta")));
    fs::remove_file(env.root().join("skills/beta/SKILL.md")).unwrap();
    env.cmd()
        .arg("validate")
        .assert()
        .code(1)
        .stdout(contains("[FAIL] beta"));
}

#[test]
fn invalid_skill_blocks_install_without_writing() {
    let env = Env::new();
    fs::remove_file(env.root().join("skills/alpha/SKILL.md")).unwrap();
    env.cmd()
        .args(["install", "alpha", "--host", "claude"])
        .assert()
        .code(1)
        .stdout(contains("[BLOCKED] alpha: validation failed"));
    assert!(!env.host("claude").join("alpha").exists());
}

#[test]
fn audit_json_reports_not_blocked_for_clean_skills() {
    let env = Env::new();
    let out = env.cmd().args(["audit", "--json"]).output().unwrap();
    assert!(out.status.success());
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["blocked"], false);
    assert_eq!(value["skills"].as_array().unwrap().len(), 2);
}

#[test]
fn data_path_uses_xdg_data_home_and_rejects_bad_names() {
    let env = Env::new();
    env.cmd()
        .args(["data-path", "alpha", "--create"])
        .assert()
        .success()
        .stdout(contains("my-skills"));
    #[cfg(not(windows))]
    assert!(env.tmp.path().join("data/my-skills/alpha").is_dir());
    #[cfg(windows)]
    assert!(
        env.tmp
            .path()
            .join("local-app-data/my-skills/data/alpha")
            .is_dir()
    );
    env.cmd().args(["data-path", "../x"]).assert().code(2);
}

#[test]
fn set_root_records_the_registry() {
    let env = Env::new();
    let expected_display = paths::display_path(&env.root().canonicalize().unwrap());
    let result = env
        .cmd()
        .env_remove("MY_SKILLS_ROOT")
        .args(["set-root", &env.root().display().to_string()])
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&result.get_output().stdout);
    assert_eq!(
        stdout.trim(),
        format!("Active registry root set to {expected_display}")
    );
    assert!(
        !stdout.contains(r"\\?\"),
        "set-root output should not contain verbatim prefix: {stdout}"
    );
    let cached = fs::read_to_string(env.tmp.path().join("config/my-skills/root")).unwrap();
    assert!(cached.trim().ends_with("registry"), "{cached}");
}

#[test]
fn bootstrap_dry_run_previews_sync_and_hooks() {
    let env = Env::new();
    env.cmd()
        .args(["bootstrap", "--dry-run"])
        .assert()
        .success()
        .stdout(
            contains("would run: my-skills sync --yes --dry-run")
                .and(contains(
                    "would suggest: run `my-skills hooks install` to check commits",
                ))
                .and(contains("then: my-skills hooks install").not()),
        );
}

#[test]
fn bootstrap_connects_enabled_skills() {
    let env = Env::new();
    env.cmd()
        .arg("bootstrap")
        .assert()
        .success()
        .stdout(contains("Bootstrap complete"));
    assert!(is_link(&env.host("agy").join("alpha")));
}

// ---- regressions from the 2026-11-23 audit review (handoff §6) ----

fn assert_install_retries_after_state_save_failure(mode: &str, expected_mode: InstallMode) {
    let env = Env::new();
    let args = ["install", "alpha", "--host", "claude", "--mode", mode];
    let host = env.host("claude");
    fs::create_dir_all(&host).unwrap();
    let dest = host.join("alpha");
    let blocker = env.block_state_save();
    env.cmd()
        .args(args)
        .assert()
        .code(1)
        .stdout(contains("FAILED: alpha -> claude (state:"))
        .stdout(contains("state.json.tmp"));
    assert_eq!(fs::read_dir(&host).unwrap().count(), 0);
    assert!(env.state().records().next().is_none());
    assert!(!env.state_path().exists());
    assert_eq!(fs::read_to_string(&blocker).unwrap(), "busy");

    fs::remove_file(blocker).unwrap();
    env.cmd()
        .args(args)
        .assert()
        .success()
        .stdout(contains("created: alpha -> claude"));
    assert!(dest.join("SKILL.md").is_file());
    assert_eq!(is_link(&dest), expected_mode == InstallMode::Link);
    assert_eq!(
        env.state().get("alpha", "claude").unwrap().mode,
        expected_mode
    );
}

#[test]
fn link_install_retries_after_state_save_failure() {
    assert_install_retries_after_state_save_failure("link", InstallMode::Link);
}

#[test]
fn copy_install_retries_after_state_save_failure() {
    assert_install_retries_after_state_save_failure("copy", InstallMode::Copy);
}

#[test]
fn copy_update_retries_after_state_save_failure() {
    let env = Env::new();
    env.cmd()
        .args(["install", "alpha", "--host", "claude", "--mode", "copy"])
        .assert()
        .success();
    let host = env.host("claude");
    let dest = host.join("alpha");
    let old_skill = fs::read(dest.join("SKILL.md")).unwrap();
    let old_state = fs::read(env.state_path()).unwrap();
    // Ignored files do not count as drift, but are part of the old copy.
    fs::write(dest.join(".DS_Store"), "local").unwrap();
    env.write_skill("alpha", "Updated alpha.");
    let blocker = env.block_state_save();
    let args = ["sync", "alpha", "--host", "claude"];
    env.cmd()
        .args(args)
        .assert()
        .code(1)
        .stdout(contains("FAILED: alpha -> claude (state:"));
    assert_eq!(fs::read(dest.join("SKILL.md")).unwrap(), old_skill);
    assert_eq!(fs::read_to_string(dest.join(".DS_Store")).unwrap(), "local");
    assert_eq!(fs::read(env.state_path()).unwrap(), old_state);
    assert_eq!(fs::read_dir(host).unwrap().count(), 1);

    fs::remove_file(blocker).unwrap();
    env.cmd()
        .args(args)
        .assert()
        .success()
        .stdout(contains("updated: alpha -> claude"));
    assert_eq!(
        fs::read(dest.join("SKILL.md")).unwrap(),
        fs::read(env.root().join("skills/alpha/SKILL.md")).unwrap(),
    );
    assert_ne!(fs::read(env.state_path()).unwrap(), old_state);
}

#[test]
fn link_uninstall_retries_after_state_save_failure() {
    let env = Env::new();
    env.cmd()
        .args(["install", "alpha", "--host", "claude", "--mode", "link"])
        .assert()
        .success();
    let host = env.host("claude");
    let dest = host.join("alpha");
    let old_state = fs::read(env.state_path()).unwrap();
    // A broken link must be restored even when the canonical path is absent.
    let source = env.root().join("skills/alpha");
    let moved_source = source.with_file_name("moved-alpha");
    fs::rename(&source, &moved_source).unwrap();
    let old_kind = link::entry_kind(&dest).unwrap();
    let old_target = link::link_target(&dest).unwrap();
    let blocker = env.block_state_save();
    let args = ["uninstall", "alpha", "--host", "claude"];
    env.cmd()
        .args(args)
        .assert()
        .code(2)
        .stderr(contains("error:").and(contains("state.json.tmp")));
    assert_eq!(link::entry_kind(&dest).unwrap(), old_kind);
    assert_eq!(link::link_target(&dest).unwrap(), old_target);
    assert_eq!(fs::read(env.state_path()).unwrap(), old_state);
    assert_eq!(fs::read_dir(&host).unwrap().count(), 1);

    fs::remove_file(blocker).unwrap();
    env.cmd()
        .args(args)
        .assert()
        .success()
        .stdout(contains("removed: alpha -> claude"));
    assert_eq!(link::entry_kind(&dest).unwrap(), EntryKind::Missing);
    assert_eq!(fs::read_dir(&host).unwrap().count(), 0);
    assert!(env.state().get("alpha", "claude").is_none());
    assert!(moved_source.join("SKILL.md").is_file());
}

#[test]
fn copy_prune_retries_after_state_save_failure() {
    let env = Env::new();
    env.cmd()
        .args(["install", "alpha", "--host", "claude", "--mode", "copy"])
        .assert()
        .success();
    let host = env.host("claude");
    let dest = host.join("alpha");
    let old_skill = fs::read(dest.join("SKILL.md")).unwrap();
    let old_state = fs::read(env.state_path()).unwrap();
    env.write_manifest(false);
    let blocker = env.block_state_save();
    let args = ["sync", "alpha", "--host", "claude"];
    env.cmd()
        .args(args)
        .assert()
        .code(1)
        .stdout(contains("FAILED: alpha -> claude (state:"));
    assert_eq!(fs::read(dest.join("SKILL.md")).unwrap(), old_skill);
    assert_eq!(fs::read(env.state_path()).unwrap(), old_state);
    assert_eq!(fs::read_dir(&host).unwrap().count(), 1);

    fs::remove_file(blocker).unwrap();
    env.cmd()
        .args(args)
        .assert()
        .success()
        .stdout(contains("removed: alpha -> claude"));
    assert_eq!(fs::read_dir(&host).unwrap().count(), 0);
    assert!(env.state().get("alpha", "claude").is_none());
    assert!(env.root().join("skills/alpha/SKILL.md").is_file());
}

#[test]
fn uninstall_retries_record_removal_when_host_entry_is_already_missing() {
    let env = Env::new();
    env.cmd()
        .args(["install", "alpha", "--host", "claude", "--mode", "link"])
        .assert()
        .success();
    let dest = env.host("claude").join("alpha");
    link::remove_link(&dest).unwrap();
    let old_state = fs::read(env.state_path()).unwrap();
    let blocker = env.block_state_save();
    let args = ["uninstall", "alpha", "--host", "claude"];
    env.cmd()
        .args(args)
        .assert()
        .code(2)
        .stderr(contains("error:").and(contains("state.json.tmp")));
    assert_eq!(link::entry_kind(&dest).unwrap(), EntryKind::Missing);
    assert_eq!(fs::read(env.state_path()).unwrap(), old_state);

    fs::remove_file(blocker).unwrap();
    env.cmd()
        .args(args)
        .assert()
        .success()
        .stdout(contains("removed: alpha -> claude"));
    assert_eq!(link::entry_kind(&dest).unwrap(), EntryKind::Missing);
    assert!(env.state().get("alpha", "claude").is_none());
}

#[test]
fn sync_check_reports_prune_drift() {
    let env = Env::new();
    env.cmd().args(["sync", "--yes"]).assert().success();
    env.write_manifest(false);
    env.cmd()
        .args(["sync", "--check"])
        .assert()
        .code(1)
        .stdout(contains("PRUNE"));
    env.cmd().args(["sync", "--yes"]).assert().success();
    env.cmd().args(["sync", "--check"]).assert().success();
}

#[test]
fn convert_rolls_back_when_state_cannot_be_saved() {
    let env = Env::new();
    env.cmd()
        .args(["install", "--mode", "copy", "--yes"])
        .assert()
        .success();
    // A leftover temp file makes the state writer refuse to save.
    let tmp = env.tmp.path().join("state/my-skills/state.json.tmp");
    fs::write(&tmp, "busy").unwrap();
    env.cmd()
        .args(["convert", "--to", "link", "alpha", "--host", "claude"])
        .assert()
        .code(1)
        .stdout(contains("FAILED: alpha -> claude"));
    let copy = env.host("claude").join("alpha");
    assert!(
        copy.join("SKILL.md").is_file() && !is_link(&copy),
        "copy restored"
    );
    fs::remove_file(&tmp).unwrap();
    env.cmd()
        .arg("status")
        .assert()
        .stdout(contains("claude   FRESH"));
    env.cmd()
        .args(["convert", "--to", "link", "alpha", "--host", "claude"])
        .assert()
        .success();
    assert!(is_link(&copy));
}

#[test]
fn convert_blocks_when_canonical_is_missing() {
    let env = Env::new();
    env.cmd()
        .args(["install", "--mode", "copy", "--yes"])
        .assert()
        .success();
    fs::remove_dir_all(env.root().join("skills/alpha")).unwrap();
    env.cmd()
        .args(["convert", "--to", "link", "alpha", "--dry-run"])
        .assert()
        .success()
        .stdout(contains("BLOCK_DRIFT").and(contains("canonical skill directory is missing")));
}

#[test]
fn doctor_full_validation_does_not_fail_for_host_display_warnings() {
    let env = Env::new();
    env.write_skill("alpha", &"x".repeat(2000));
    fs::remove_file(env.root().join("skills/beta/SKILL.md")).unwrap();
    let out = env
        .cmd()
        .args(["doctor", "--no-update-check", "--json"])
        .output()
        .unwrap();
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(out.status.success());
    assert_eq!(
        report["full_validation"],
        serde_json::json!({"checked": 2, "failures": [
            {"skill": "beta", "errors": ["SKILL.md not found"]}
        ]})
    );
}

#[test]
fn bootstrap_dry_run_shows_mode_override() {
    let env = Env::new();
    env.cmd()
        .args(["bootstrap", "--dry-run", "--mode", "copy"])
        .assert()
        .success()
        .stdout(contains("with install mode: copy"));
}

#[test]
fn only_hosts_a_skill_connects_to_report_display_warnings() {
    let env = Env::new();
    // beta is claude-only, despite exceeding both measured display limits.
    env.write_skill("beta", &"x".repeat(2000));
    env.cmd()
        .args(["install", "beta", "--host", "codex"])
        .assert()
        .success()
        .stdout(contains("skipped: beta -> codex").and(contains("shows a shortened").not()));
    env.cmd()
        .args(["install", "beta", "--host", "claude"])
        .assert()
        .success()
        .stdout(contains("Claude Code shows a shortened").and(contains("Codex").not()));
}

#[test]
fn install_and_sync_respect_cli_target_defaults_precedence_and_keep_recorded_mode() {
    let env = Env::new();
    fs::write(
        env.root().join("my-skills.local.toml"),
        "[defaults]\ninstall_mode = \"link\"\n\n[targets.agy]\ninstall_mode = \"copy\"\n",
    )
    .unwrap();

    let create_actions = |value: &serde_json::Value| -> Vec<(String, String, String)> {
        let mut creates: Vec<_> = value["actions"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|a| a["action"] == "CREATE")
            .map(|a| {
                (
                    a["skill"].as_str().unwrap().to_owned(),
                    a["host"].as_str().unwrap().to_owned(),
                    a["mode"].as_str().unwrap().to_owned(),
                )
            })
            .collect();
        creates.sort();
        creates
    };

    // Without CLI --mode: agy uses target copy, others use default link.
    let out = env
        .cmd()
        .args(["install", "--dry-run", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        create_actions(&value),
        vec![
            ("alpha".into(), "agy".into(), "copy".into()),
            ("alpha".into(), "claude".into(), "link".into()),
            ("alpha".into(), "codex".into(), "link".into()),
            ("beta".into(), "claude".into(), "link".into()),
        ],
        "{value}"
    );

    // CLI --mode link overrides [targets.agy] install_mode = "copy".
    let out = env
        .cmd()
        .args(["install", "--mode", "link", "--dry-run", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        create_actions(&value),
        vec![
            ("alpha".into(), "agy".into(), "link".into()),
            ("alpha".into(), "claude".into(), "link".into()),
            ("alpha".into(), "codex".into(), "link".into()),
            ("beta".into(), "claude".into(), "link".into()),
        ],
        "{value}"
    );

    // CLI --mode copy overrides [defaults] install_mode = "link".
    let out = env
        .cmd()
        .args(["install", "--mode", "copy", "--dry-run", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        create_actions(&value),
        vec![
            ("alpha".into(), "agy".into(), "copy".into()),
            ("alpha".into(), "claude".into(), "copy".into()),
            ("alpha".into(), "codex".into(), "copy".into()),
            ("beta".into(), "claude".into(), "copy".into()),
        ],
        "{value}"
    );

    // sync connects claude/codex as links and agy as a copy.
    env.cmd().args(["sync", "--yes"]).assert().success();
    assert!(is_link(&env.host("claude").join("alpha")));
    assert!(is_link(&env.host("codex").join("alpha")));
    let agy_alpha = env.host("agy").join("alpha");
    assert!(agy_alpha.is_dir() && !is_link(&agy_alpha));

    // Recorded connections retain their mode even if target config or CLI --mode changes.
    fs::write(
        env.root().join("my-skills.local.toml"),
        "[defaults]\ninstall_mode = \"link\"\n\n[targets.agy]\ninstall_mode = \"link\"\n",
    )
    .unwrap();
    env.cmd()
        .args(["install", "alpha", "--host", "agy", "--mode", "link"])
        .assert()
        .success()
        .stdout(contains("unchanged: alpha -> agy"));
    assert!(agy_alpha.is_dir() && !is_link(&agy_alpha));
}

#[test]
fn convert_host_all_skips_explicit_differing_target_mode_and_explicit_host_converts() {
    let env = Env::new();
    // Start with all connections in copy mode.
    env.cmd()
        .args(["install", "--mode", "copy", "--yes"])
        .assert()
        .success();
    // Pin agy to copy mode in local overlay.
    fs::write(
        env.root().join("my-skills.local.toml"),
        "[targets.agy]\ninstall_mode = \"copy\"\n",
    )
    .unwrap();

    // Dry run with --host all skips agy and prints the informational line.
    let dry = env
        .cmd()
        .args(["convert", "--to", "link", "--host", "all", "--dry-run"])
        .output()
        .unwrap();
    assert!(dry.status.success());
    let dry_stdout = String::from_utf8_lossy(&dry.stdout);
    assert!(
        dry_stdout.contains("CONVERT          alpha -> claude"),
        "{dry_stdout}"
    );
    assert!(
        dry_stdout.contains("CONVERT          alpha -> codex"),
        "{dry_stdout}"
    );
    assert!(
        !dry_stdout.contains("CONVERT          alpha -> agy"),
        "{dry_stdout}"
    );
    assert!(
        dry_stdout.contains("skipped host agy ([targets.agy] install_mode = \"copy\")"),
        "{dry_stdout}"
    );

    // Apply with --host all skips agy and converts claude and codex.
    let apply = env
        .cmd()
        .args(["convert", "--to", "link", "--host", "all", "--yes"])
        .output()
        .unwrap();
    assert!(apply.status.success());
    let apply_stdout = String::from_utf8_lossy(&apply.stdout);
    assert!(
        apply_stdout.contains("skipped host agy ([targets.agy] install_mode = \"copy\")"),
        "{apply_stdout}"
    );
    assert!(is_link(&env.host("claude").join("alpha")));
    assert!(is_link(&env.host("codex").join("alpha")));
    let agy_alpha = env.host("agy").join("alpha");
    assert!(agy_alpha.is_dir() && !is_link(&agy_alpha));

    // Omitting --host behaves the same as --host all.
    env.cmd()
        .args(["convert", "--to", "link", "--dry-run"])
        .assert()
        .success()
        .stdout(contains(
            "skipped host agy ([targets.agy] install_mode = \"copy\")",
        ));

    // Explicit --host agy converts agy as requested.
    env.cmd()
        .args(["convert", "--to", "link", "--host", "agy", "--dry-run"])
        .assert()
        .success()
        .stdout(contains("CONVERT          alpha -> agy"))
        .stdout(predicate::str::contains("skipped host agy").not());
    env.cmd()
        .args(["convert", "--to", "link", "--host", "agy"])
        .assert()
        .success()
        .stdout(contains(format!(
            "converted: alpha -> agy (link: {LINK_KIND})"
        )));
    assert!(is_link(&agy_alpha));
}

#[test]
fn cli_target_link_overrides_defaults_copy() {
    let env = Env::new();
    fs::write(
        env.root().join("my-skills.local.toml"),
        "[defaults]\ninstall_mode = \"copy\"\n\n[targets.claude]\ninstall_mode = \"link\"\n",
    )
    .unwrap();

    let out = env
        .cmd()
        .args(["install", "--dry-run", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let mut creates: Vec<_> = value["actions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|a| a["action"] == "CREATE")
        .map(|a| {
            (
                a["skill"].as_str().unwrap().to_owned(),
                a["host"].as_str().unwrap().to_owned(),
                a["mode"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    creates.sort();
    assert_eq!(
        creates,
        vec![
            ("alpha".into(), "agy".into(), "copy".into()),
            ("alpha".into(), "claude".into(), "link".into()),
            ("alpha".into(), "codex".into(), "copy".into()),
            ("beta".into(), "claude".into(), "link".into()),
        ],
        "{value}"
    );

    env.cmd().args(["install", "--yes"]).assert().success();
    assert!(is_link(&env.host("claude").join("alpha")));
    assert!(is_link(&env.host("claude").join("beta")));
    let codex_alpha = env.host("codex").join("alpha");
    let agy_alpha = env.host("agy").join("alpha");
    assert!(codex_alpha.is_dir() && !is_link(&codex_alpha));
    assert!(agy_alpha.is_dir() && !is_link(&agy_alpha));
}

#[test]
fn convert_to_copy_host_all_skips_link_pinned_host() {
    let env = Env::new();
    env.cmd().args(["sync", "--yes"]).assert().success();
    fs::write(
        env.root().join("my-skills.local.toml"),
        "[targets.claude]\ninstall_mode = \"link\"\n",
    )
    .unwrap();

    // Dry run with --to copy --host all skips claude and plans codex/agy.
    let dry = env
        .cmd()
        .args(["convert", "--to", "copy", "--host", "all", "--dry-run"])
        .output()
        .unwrap();
    assert!(dry.status.success());
    let dry_stdout = String::from_utf8_lossy(&dry.stdout);
    assert!(
        dry_stdout.contains("CONVERT          alpha -> codex"),
        "{dry_stdout}"
    );
    assert!(
        dry_stdout.contains("CONVERT          alpha -> agy"),
        "{dry_stdout}"
    );
    assert!(
        !dry_stdout.contains("CONVERT          alpha -> claude"),
        "{dry_stdout}"
    );
    assert!(
        dry_stdout.contains("skipped host claude ([targets.claude] install_mode = \"link\")"),
        "{dry_stdout}"
    );

    // Apply with --to copy --host all converts codex/agy and leaves claude linked.
    let apply = env
        .cmd()
        .args(["convert", "--to", "copy", "--host", "all", "--yes"])
        .output()
        .unwrap();
    assert!(apply.status.success());
    let apply_stdout = String::from_utf8_lossy(&apply.stdout);
    assert!(
        apply_stdout.contains("skipped host claude ([targets.claude] install_mode = \"link\")"),
        "{apply_stdout}"
    );
    assert!(is_link(&env.host("claude").join("alpha")));
    let codex_alpha = env.host("codex").join("alpha");
    let agy_alpha = env.host("agy").join("alpha");
    assert!(codex_alpha.is_dir() && !is_link(&codex_alpha));
    assert!(agy_alpha.is_dir() && !is_link(&agy_alpha));

    // Explicit --host claude still converts claude to copy.
    env.cmd()
        .args(["convert", "--to", "copy", "--host", "claude"])
        .assert()
        .success()
        .stdout(contains("converted: alpha -> claude"))
        .stdout(predicate::str::contains("skipped host claude").not());
    let claude_alpha = env.host("claude").join("alpha");
    assert!(claude_alpha.is_dir() && !is_link(&claude_alpha));
}
