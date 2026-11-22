//! Every write command supports `--dry-run` (spec §6) and leaves the
//! filesystem unchanged (handoff §7 B).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;

use assert_cmd::Command;
use predicates::str::contains;

struct Env {
    tmp: tempfile::TempDir,
}

impl Env {
    fn new() -> Self {
        let env = Env {
            tmp: tempfile::tempdir().unwrap(),
        };
        let skill = env.root().join("skills/alpha");
        fs::create_dir_all(&skill).unwrap();
        fs::write(
            skill.join("SKILL.md"),
            "---\nname: alpha\ndescription: Demo skill alpha.\n---\n\nBody.\n",
        )
        .unwrap();
        let mut toml = String::from("schema_version = 1\nskills_root = \"skills\"\n");
        for host in ["claude", "codex", "agy", "hermes"] {
            toml.push_str(&format!(
                "\n[targets.{host}]\nenabled = {}\npath = '{}'\n",
                host != "hermes",
                env.tmp.path().join("hosts").join(host).display()
            ));
        }
        toml.push_str("\n[skills.alpha]\nenabled = true\n");
        fs::write(env.root().join("my-skills.toml"), toml).unwrap();
        env
    }

    fn root(&self) -> PathBuf {
        self.tmp.path().join("registry")
    }

    fn manifest(&self) -> String {
        fs::read_to_string(self.root().join("my-skills.toml")).unwrap()
    }

    fn cmd(&self) -> Command {
        let base = self.tmp.path();
        let mut cmd = Command::cargo_bin("my-skills").unwrap();
        cmd.current_dir(base)
            .env("HOME", base.join("home"))
            .env("XDG_STATE_HOME", base.join("state"))
            .env("XDG_DATA_HOME", base.join("data"))
            .env("XDG_CONFIG_HOME", base.join("config"))
            .env("MY_SKILLS_ROOT", self.root())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", base.join("gitconfig"));
        cmd
    }

    fn git(&self, args: &[&str]) {
        let status = StdCommand::new("git")
            .args(args)
            .current_dir(self.root())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", self.tmp.path().join("gitconfig"))
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?}");
    }
}

fn listing(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = match fs::read_dir(dir) {
        Ok(entries) => entries
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect(),
        Err(_) => Vec::new(),
    };
    names.sort();
    names
}

#[test]
fn enable_and_disable_dry_run_leave_the_manifest_alone() {
    let env = Env::new();
    let before = env.manifest();
    env.cmd()
        .args(["disable", "alpha", "--dry-run"])
        .assert()
        .success()
        .stdout(contains("would mark alpha disabled (currently enabled)"));
    env.cmd()
        .args(["enable", "alpha", "--dry-run"])
        .assert()
        .success();
    assert_eq!(env.manifest(), before);
    env.cmd()
        .args(["disable", "nope", "--dry-run"])
        .assert()
        .code(2);
}

#[test]
fn set_root_dry_run_does_not_write_the_cache() {
    let env = Env::new();
    env.cmd()
        .env_remove("MY_SKILLS_ROOT")
        .args(["set-root", &env.root().display().to_string(), "--dry-run"])
        .assert()
        .success()
        .stdout(contains("would set active registry root"));
    assert!(!env.tmp.path().join("config/my-skills/root").exists());
}

#[test]
fn init_registry_dry_run_creates_nothing() {
    let env = Env::new();
    let target = env.tmp.path().join("new-registry");
    env.cmd()
        .args(["init-registry", &target.display().to_string(), "--dry-run"])
        .assert()
        .success()
        .stdout(contains("would create a private skill registry"));
    assert!(!target.exists());
}

#[test]
fn import_dry_run_validates_but_writes_nothing() {
    let env = Env::new();
    let source = env.tmp.path().join("incoming/beta");
    fs::create_dir_all(&source).unwrap();
    fs::write(
        source.join("SKILL.md"),
        "---\nname: beta\ndescription: Incoming skill.\n---\n\nBody.\n",
    )
    .unwrap();
    let before_manifest = env.manifest();
    let before_skills = listing(&env.root().join("skills"));
    env.cmd()
        .args(["import", &source.display().to_string(), "--dry-run"])
        .assert()
        .success()
        .stdout(contains("would import to"));
    assert_eq!(env.manifest(), before_manifest);
    assert_eq!(listing(&env.root().join("skills")), before_skills);
    assert!(
        !listing(&env.root()).iter().any(|n| n.contains("edit-lock")),
        "lock released"
    );
}

#[test]
fn hooks_uninstall_dry_run_keeps_hooks() {
    let env = Env::new();
    env.git(&["init", "-q"]);
    env.cmd().args(["hooks", "install"]).assert().success();
    let hooks = env.root().join(".git/hooks");
    let before = listing(&hooks);
    env.cmd()
        .args(["hooks", "uninstall", "--dry-run"])
        .assert()
        .success()
        .stdout(contains("would remove:"));
    assert_eq!(listing(&hooks), before);
    assert!(hooks.join("pre-commit").is_file());
}

#[test]
fn share_dry_run_is_a_plan() {
    let env = Env::new();
    env.cmd()
        .args(["share", "--from", "claude", "--dry-run", "--json"])
        .assert()
        .success();
    assert_eq!(
        listing(&env.root().join("skills")),
        vec!["alpha".to_string()]
    );
}
