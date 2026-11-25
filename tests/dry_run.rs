//! Every write command supports `--dry-run` (spec §6) and leaves the
//! filesystem unchanged (handoff §7 B).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;

use assert_cmd::Command;
use my_skills::platform::paths;
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
        for host in ["claude", "codex", "agy", "custom"] {
            toml.push_str(&format!(
                "\n[targets.{host}]\nenabled = {}\npath = '{}'\n",
                host != "custom",
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

    fn root_cache(&self) -> PathBuf {
        self.tmp.path().join("config/my-skills/root")
    }

    fn manifest(&self) -> String {
        fs::read_to_string(self.root().join("my-skills.toml")).unwrap()
    }

    fn cmd_in_registry_without_env(&self) -> Command {
        let mut cmd = self.cmd();
        cmd.current_dir(self.root()).env_remove("MY_SKILLS_ROOT");
        cmd
    }

    fn cmd(&self) -> Command {
        let base = self.tmp.path();
        let mut cmd = Command::cargo_bin("my-skills").unwrap();
        cmd.current_dir(base)
            .env("HOME", base.join("home"))
            .env("USERPROFILE", base.join("home"))
            .env("LOCALAPPDATA", base.join("local-app-data"))
            .env("APPDATA", base.join("appdata"))
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
    let expected_display = paths::display_path(&env.root().canonicalize().unwrap());
    let result = env
        .cmd()
        .env_remove("MY_SKILLS_ROOT")
        .args(["set-root", &env.root().display().to_string(), "--dry-run"])
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&result.get_output().stdout);
    assert_eq!(
        stdout.trim(),
        format!("Dry run — would set active registry root to {expected_display}")
    );
    assert!(
        !stdout.contains(r"\\?\"),
        "set-root --dry-run output should not contain verbatim prefix: {stdout}"
    );
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

#[test]
fn dry_run_from_cwd_without_env_or_cache_leaves_config_home_empty() {
    let env = Env::new();
    env.git(&["init", "-q"]);
    let incoming = env.tmp.path().join("incoming/beta");
    fs::create_dir_all(&incoming).unwrap();
    fs::write(
        incoming.join("SKILL.md"),
        "---\nname: beta\ndescription: Incoming skill.\n---\n\nBody.\n",
    )
    .unwrap();

    let before_manifest = env.manifest();
    let expected_display = paths::display_path(&env.root().canonicalize().unwrap());
    let expected_preview = format!("would remember {expected_display} as the active registry");
    let incoming_str = incoming.display().to_string();

    let manifest_dry_runs: &[&[&str]] = &[
        &["install", "--dry-run"],
        &["sync", "--dry-run"],
        &["uninstall", "alpha", "--dry-run"],
        &["convert", "--to", "copy", "--dry-run"],
        &["enable", "alpha", "--dry-run"],
        &["disable", "alpha", "--dry-run"],
        &["import", &incoming_str, "--dry-run"],
        &["share", "--from", "claude", "--dry-run"],
    ];

    for args in manifest_dry_runs {
        let out = env
            .cmd_in_registry_without_env()
            .args(*args)
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            out.status.success(),
            "args={args:?} failed:\nstdout={stdout}\nstderr={stderr}"
        );
        assert!(
            !env.root_cache().exists(),
            "args={args:?} wrote root cache at {}",
            env.root_cache().display()
        );
        assert!(
            !env.tmp.path().join("config").exists(),
            "args={args:?} created config directory"
        );
        assert!(
            stdout.contains(&expected_preview),
            "args={args:?} missing preview `{expected_preview}` in:\n{stdout}"
        );
        assert!(
            !stdout.contains(r"\\?\"),
            "args={args:?} printed verbatim prefix:\n{stdout}"
        );
    }

    for args in [
        &["install", "--dry-run", "--json"][..],
        &["share", "--from", "claude", "--dry-run", "--json"][..],
    ] {
        let out = env
            .cmd_in_registry_without_env()
            .args(args)
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            out.status.success(),
            "args={args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_str::<serde_json::Value>(&stdout)
            .unwrap_or_else(|e| panic!("args={args:?} emitted invalid JSON ({e}):\n{stdout}"));
        assert!(!env.root_cache().exists(), "args={args:?} wrote root cache");
    }

    for args in [
        &["bootstrap", "--dry-run"][..],
        &["set-root", "--dry-run"][..],
        &["hooks", "install", "--dry-run"][..],
        &["hooks", "uninstall", "--dry-run"][..],
    ] {
        env.cmd_in_registry_without_env()
            .args(args)
            .assert()
            .success();
        assert!(!env.root_cache().exists(), "args={args:?} wrote root cache");
    }

    assert_eq!(env.manifest(), before_manifest);
    assert!(!env.tmp.path().join("config").exists());
    assert!(!env.tmp.path().join("state").exists());
    assert!(!env.tmp.path().join("hosts").exists());
}

#[test]
fn dry_run_does_not_preview_or_repoint_when_cache_already_exists() {
    let env = Env::new();
    let other = env.tmp.path().join("other-registry");
    fs::create_dir_all(&other).unwrap();
    fs::write(other.join("my-skills.toml"), "schema_version = 1\n").unwrap();
    let other_canonical = other.canonicalize().unwrap();
    fs::create_dir_all(env.root_cache().parent().unwrap()).unwrap();
    let before_bytes = format!("{}\n", other_canonical.display()).into_bytes();
    fs::write(env.root_cache(), &before_bytes).unwrap();

    let out = env
        .cmd_in_registry_without_env()
        .args(["install", "--dry-run"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    assert!(
        stderr.contains("note: using ./my-skills.toml for this command; active registry is"),
        "expected current-directory registry note on stderr:\n{stderr}"
    );
    assert!(
        !stdout.contains("would remember"),
        "should not preview caching when a cache already exists:\n{stdout}"
    );
    assert_eq!(fs::read(env.root_cache()).unwrap(), before_bytes);

    let json_out = env
        .cmd_in_registry_without_env()
        .args([
            "install",
            "alpha",
            "--host",
            "claude",
            "--dry-run",
            "--json",
        ])
        .output()
        .unwrap();
    let json_stdout = String::from_utf8_lossy(&json_out.stdout);
    let json_stderr = String::from_utf8_lossy(&json_out.stderr);
    assert!(json_out.status.success(), "{json_stderr}");
    assert!(
        json_stderr.contains("note: using ./my-skills.toml for this command; active registry is"),
        "expected current-directory registry note on stderr for --json:\n{json_stderr}"
    );
    let parsed: serde_json::Value = serde_json::from_str(&json_stdout)
        .unwrap_or_else(|e| panic!("invalid JSON stdout ({e}):\n{json_stdout}"));
    let actions = parsed["actions"].as_array().unwrap();
    assert_eq!(actions.len(), 1, "{parsed}");
    assert_eq!(actions[0]["skill"], "alpha");
    assert_eq!(actions[0]["host"], "claude");
    assert_eq!(fs::read(env.root_cache()).unwrap(), before_bytes);
}

#[test]
fn real_command_from_cwd_writes_root_cache_and_never_repoints_existing_cache() {
    let env = Env::new();
    let expected_root = env.root().canonicalize().unwrap();

    env.cmd_in_registry_without_env()
        .args(["install", "alpha", "--host", "claude"])
        .assert()
        .success();
    let cached = fs::read_to_string(env.root_cache()).unwrap();
    assert_eq!(cached.trim(), expected_root.display().to_string());

    let other = env.tmp.path().join("other-registry");
    fs::create_dir_all(&other).unwrap();
    fs::write(other.join("my-skills.toml"), "schema_version = 1\n").unwrap();

    env.cmd()
        .current_dir(&other)
        .env_remove("MY_SKILLS_ROOT")
        .arg("status")
        .assert()
        .success()
        .stderr(contains("note: using ./my-skills.toml for this command"));
    let cached_after = fs::read_to_string(env.root_cache()).unwrap();
    assert_eq!(cached_after.trim(), expected_root.display().to_string());
}

#[test]
fn import_dry_run_writes_no_root_cache_and_respects_edit_lock() {
    let env = Env::new();
    let incoming = env.tmp.path().join("incoming/beta");
    fs::create_dir_all(&incoming).unwrap();
    fs::write(
        incoming.join("SKILL.md"),
        "---\nname: beta\ndescription: Incoming skill.\n---\n\nBody.\n",
    )
    .unwrap();
    let before_manifest = env.manifest();
    let before_skills = listing(&env.root().join("skills"));
    let lock = env.root().join("my-skills.toml.edit-lock");
    fs::write(&lock, "other writer\n").unwrap();

    env.cmd_in_registry_without_env()
        .args(["import", &incoming.display().to_string(), "--dry-run"])
        .assert()
        .code(2)
        .stderr(contains("my-skills.toml.edit-lock"));
    assert_eq!(fs::read_to_string(&lock).unwrap(), "other writer\n");
    assert!(!env.root_cache().exists());
    assert!(!env.tmp.path().join("config").exists());
    assert_eq!(listing(&env.root().join("skills")), before_skills);

    fs::remove_file(&lock).unwrap();
    let before_registry = listing(&env.root());

    env.cmd_in_registry_without_env()
        .args(["import", &incoming.display().to_string(), "--dry-run"])
        .assert()
        .success()
        .stdout(contains("would import to"))
        .stdout(contains("as the active registry"));
    assert!(!env.root_cache().exists());
    assert!(!env.tmp.path().join("config").exists());
    assert_eq!(env.manifest(), before_manifest);
    assert_eq!(listing(&env.root()), before_registry);
    assert_eq!(listing(&env.root().join("skills")), before_skills);
}

#[test]
fn import_dry_run_warns_about_a_missing_staged_reference_like_real_import() {
    let env = Env::new();
    let incoming = env.tmp.path().join("incoming/beta");
    fs::create_dir_all(incoming.join("sub")).unwrap();
    fs::write(env.tmp.path().join("incoming/notes.txt"), "outside\n").unwrap();
    fs::write(
        incoming.join("SKILL.md"),
        "---\nname: beta\ndescription: Escaping reference test.\n---\n\nSee [Notes](sub/../../notes.txt).\n",
    )
    .unwrap();

    let warning = "Referenced supporting file not found: sub/../../notes.txt";
    let before_manifest = env.manifest();
    let before_registry = listing(&env.root());
    let before_skills = listing(&env.root().join("skills"));

    env.cmd_in_registry_without_env()
        .args(["import", &incoming.display().to_string(), "--dry-run"])
        .assert()
        .success()
        .stdout(contains(warning));
    assert!(!env.root_cache().exists());
    assert!(!env.tmp.path().join("config").exists());
    assert_eq!(env.manifest(), before_manifest);
    assert_eq!(listing(&env.root()), before_registry);
    assert_eq!(listing(&env.root().join("skills")), before_skills);

    env.cmd()
        .args(["import", &incoming.display().to_string()])
        .assert()
        .success()
        .stdout(contains(warning));
    assert_eq!(
        fs::read(env.root().join("skills/beta/SKILL.md")).unwrap(),
        fs::read(incoming.join("SKILL.md")).unwrap()
    );
}

#[cfg(windows)]
#[test]
fn import_dry_run_fails_on_locked_git_file_like_real_import() {
    use std::os::windows::fs::OpenOptionsExt;

    let env = Env::new();
    let incoming = env.tmp.path().join("incoming/beta");
    fs::create_dir_all(incoming.join(".git")).unwrap();
    fs::write(
        incoming.join("SKILL.md"),
        "---\nname: beta\ndescription: Locked git file test.\n---\n\nBody.\n",
    )
    .unwrap();
    let locked_path = incoming.join(".git/copy-me.txt");
    fs::write(&locked_path, "locked\n").unwrap();

    let _held = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(0)
        .open(&locked_path)
        .unwrap();

    let before_manifest = env.manifest();
    let before_registry = listing(&env.root());
    let before_skills = listing(&env.root().join("skills"));

    let dry = env
        .cmd_in_registry_without_env()
        .args(["import", &incoming.display().to_string(), "--dry-run"])
        .output()
        .unwrap();
    let dry_stderr = String::from_utf8_lossy(&dry.stderr);
    assert_eq!(dry.status.code(), Some(2), "stderr={dry_stderr}");
    assert!(dry_stderr.contains("copy-me.txt"), "stderr={dry_stderr}");
    assert!(!env.root_cache().exists());
    assert!(!env.tmp.path().join("config").exists());
    assert_eq!(env.manifest(), before_manifest);
    assert_eq!(listing(&env.root()), before_registry);
    assert_eq!(listing(&env.root().join("skills")), before_skills);

    let real = env
        .cmd()
        .args(["import", &incoming.display().to_string()])
        .output()
        .unwrap();
    let real_stderr = String::from_utf8_lossy(&real.stderr);
    assert_eq!(real.status.code(), Some(2), "stderr={real_stderr}");
    assert!(real_stderr.contains("copy-me.txt"), "stderr={real_stderr}");
}
