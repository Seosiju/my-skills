//! Black-box tests for the integrator's commands: install, sync, uninstall,
//! convert, bootstrap, validate, audit, status, set-root, data-path.

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
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
            ("hermes", false),
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
fn install_json_requires_dry_run() {
    let env = Env::new();
    env.cmd()
        .args(["install", "--json"])
        .assert()
        .code(2)
        .stderr(contains("install --json requires --dry-run"));
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
    env.write_skill("beta", "");
    env.cmd()
        .arg("validate")
        .assert()
        .code(1)
        .stdout(contains("[FAIL] beta"));
}

#[test]
fn invalid_skill_blocks_install_without_writing() {
    let env = Env::new();
    env.write_skill("alpha", "");
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
    env.cmd().args(["data-path", "../x"]).assert().code(2);
}

#[test]
fn set_root_records_the_registry() {
    let env = Env::new();
    env.cmd()
        .env_remove("MY_SKILLS_ROOT")
        .args(["set-root", &env.root().display().to_string()])
        .assert()
        .success()
        .stdout(contains("Active registry root set to"));
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
            contains("would run: my-skills sync --yes --dry-run").and(contains("hooks install")),
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
fn doctor_full_validation_includes_host_checks() {
    let env = Env::new();
    // Under the 1024-character standard limit, over the 768-character host limit.
    env.write_skill("alpha", &"x".repeat(800));
    let out = env
        .cmd()
        .args(["doctor", "--no-update-check", "--json"])
        .output()
        .unwrap();
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let failures = report["full_validation"]["failures"].as_array().unwrap();
    let alpha = failures.iter().find(|f| f["skill"] == "alpha").unwrap();
    assert!(
        alpha["errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e.as_str().unwrap().contains("description exceeds 768")),
        "{alpha}"
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
