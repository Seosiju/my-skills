mod support_h;
mod support_i;

use std::fs;
use std::path::Path;

use assert_cmd::Command;
use predicates::prelude::*;
use support_h::{Fixture, json, native, plain};
use support_i::assert_with_busy_retry;

fn init(fixture: &Fixture) -> Command {
    let mut cmd = fixture.command();
    cmd.arg("init-registry").arg(&fixture.registry);
    cmd
}

fn git(fixture: &Fixture, args: &[&str]) -> String {
    let output = fixture.process("git").args(args).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .unwrap()
        .replace("\r\n", "\n")
}

#[test]
fn init_registry_registers_seed_skills_in_manifest() {
    let fixture = Fixture::new();
    init(&fixture).arg("--no-git").assert().success().stdout(format!(
        "Created private skill registry at {}\nNext: cd there, add skills/<name>/SKILL.md, then run `my-skills install --dry-run`.\n",
        plain(&fixture.registry)
    ));
    let manifest = fixture.config();
    assert_eq!(manifest["defaults"]["install_mode"].as_str(), Some("link"));
    let skills = manifest["skills"].as_table().unwrap();
    assert_eq!(skills.len(), 3);
    for (name, enabled) in [
        ("cli-inventory", true),
        ("my-skills", true),
        ("my-jira", false),
    ] {
        let row = &skills[name];
        assert_eq!(row["enabled"].as_bool(), Some(enabled));
        assert_eq!(
            row["hosts"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect::<Vec<_>>(),
            ["claude", "codex", "hermes", "agy"]
        );
        assert_eq!(row["source_type"].as_str(), Some("builtin-seed"));
        assert_eq!(row["source_revision"].as_str(), Some("1.0.0-alpha.0"));
    }
}

#[test]
fn init_registry_does_not_restore_removed_skills_or_delete_local_data() {
    let fixture = Fixture::new();
    init(&fixture).arg("--no-git").assert().success();
    let removed = fixture.canonical("cli-inventory");
    fs::remove_dir_all(&removed).unwrap();
    let original = fs::read(fixture.manifest()).unwrap();
    let memory = fixture.root.join("data/my-skills/example/memory.json");
    fs::create_dir_all(memory.parent().unwrap()).unwrap();
    fs::write(&memory, "{\"keep\": true}\n").unwrap();
    init(&fixture).arg("--no-git").assert().code(1).stderr(
        "error: registry scaffold already exists: my-skills.toml, skills, .gitignore, README.md\n",
    );
    assert!(!removed.exists());
    assert_eq!(fs::read(fixture.manifest()).unwrap(), original);
    assert_eq!(fs::read_to_string(memory).unwrap(), "{\"keep\": true}\n");
}

#[test]
fn init_registry_install_dry_run_has_seed_actions() {
    let fixture = Fixture::new();
    init(&fixture).arg("--no-git").assert().success();
    let data = json(fixture.command().args(["install", "--dry-run", "--json"]));
    let names = data["actions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|action| action["skill"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(names, ["cli-inventory", "my-skills"].into_iter().collect());
}

#[test]
fn seed_default_install_requires_confirmation_for_custom_targets() {
    let fixture = Fixture::new();
    init(&fixture).arg("--no-git").assert().success();
    let local = fixture.registry.join("my-skills.local.toml");
    fs::write(local, "[targets.claude]\npath = \"custom/claude\"\n").unwrap();
    fixture
        .command()
        .arg("install")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("writes to multiple hosts"));
}

#[test]
fn seed_default_install_requires_confirmation_for_non_default_options() {
    let fixture = Fixture::new();
    init(&fixture).arg("--no-git").assert().success();
    for args in [
        &["install", "--mode", "link"][..],
        &["install", "--skip-audit"][..],
    ] {
        fixture
            .command()
            .args(args)
            .assert()
            .code(2)
            .stderr(predicate::str::contains("writes to multiple hosts"));
    }
}

#[test]
fn init_registry_no_defaults_has_no_skill_manifest_entries() {
    let fixture = Fixture::new();
    init(&fixture)
        .args(["--no-defaults", "--no-git"])
        .assert()
        .success();
    assert!(!fixture.config().contains_key("skills"));
    assert_eq!(
        fs::read_dir(fixture.registry.join("skills"))
            .unwrap()
            .count(),
        0
    );
}

#[test]
fn init_registry_caches_root_for_later_commands() {
    let fixture = Fixture::new();
    init(&fixture).arg("--no-git").assert().success();
    let data = json(
        fixture
            .command()
            .env_remove("MY_SKILLS_ROOT")
            .current_dir(fixture.root.join("elsewhere"))
            .args(["skills", "--json"]),
    );
    let names = data["skills"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(names, ["cli-inventory", "my-jira", "my-skills"]);
}

#[test]
fn init_registry_without_path_uses_default_location_when_not_tty() {
    let fixture = Fixture::new();
    let target = fixture.root.join("home/my-agent-skills");
    fixture
        .command()
        .args(["init-registry", "--no-defaults", "--no-git"])
        .assert()
        .success()
        .stdout(predicate::str::contains(format!(
            "Created private skill registry at {}",
            plain(&target)
        )));
    assert!(target.join("my-skills.toml").is_file());
}

#[test]
fn init_registry_initializes_git_repo_by_default() {
    let fixture = Fixture::new();
    init(&fixture)
        .arg("--no-defaults")
        .assert()
        .success()
        .stdout(predicate::str::starts_with(
            "Created initial registry commit\n",
        ));
    assert!(fixture.registry.join(".git").is_dir());
    assert_eq!(
        git(&fixture, &["log", "-1", "--format=%s"]),
        "chore: initialize my-skills registry\n"
    );
}

#[test]
fn init_registry_initial_commit_does_not_track_preexisting_local_state() {
    let fixture = Fixture::new();
    let state = fixture.registry.join(".omc/state/private.txt");
    fs::create_dir_all(state.parent().unwrap()).unwrap();
    fs::write(&state, "local\n").unwrap();
    init(&fixture).arg("--no-defaults").assert().success();
    assert_eq!(
        git(&fixture, &["ls-tree", "-r", "--name-only", "HEAD"]),
        ".gitignore\nREADME.md\nmy-skills.toml\n"
    );
    assert_eq!(fs::read_to_string(state).unwrap(), "local\n");
}

#[test]
fn init_registry_seed_copy_does_not_copy_private_source_files() {
    let fixture = Fixture::new();
    init(&fixture).arg("--with-defaults").assert().success();
    let seed_files = [
        "cli-inventory/SKILL.md",
        "cli-inventory/references/inventory-schema.md",
        "cli-inventory/scripts/scan_tools.py",
        "my-jira/SKILL.md",
        "my-jira/config.example.json",
        "my-skills/SKILL.md",
    ];
    let tool_repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    for file in seed_files {
        assert_eq!(
            fs::read(fixture.registry.join("skills").join(file)).unwrap(),
            fs::read(tool_repo.join("skills").join(file)).unwrap(),
        );
    }
    let expected = format!(
        ".gitignore\nREADME.md\nmy-skills.toml\n{}\n",
        seed_files.map(|file| format!("skills/{file}")).join("\n")
    );
    assert_eq!(
        git(&fixture, &["ls-tree", "-r", "--name-only", "HEAD"]),
        expected
    );
    assert!(!fixture.canonical("my-jira").join("config.json").exists());
}

#[test]
fn init_registry_no_git_skips_git_init() {
    let fixture = Fixture::new();
    init(&fixture)
        .args(["--no-defaults", "--no-git"])
        .assert()
        .success();
    assert!(!fixture.registry.join(".git").exists());
}

#[test]
fn init_registry_missing_git_is_graceful() {
    let fixture = Fixture::new();
    init(&fixture)
        .arg("--no-defaults")
        .env("PATH", fixture.root.join("bin"))
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "git command not found; skipped git init",
        ));
    assert!(!fixture.registry.join(".git").exists());
}

#[test]
fn init_registry_agy_opt_in_target() {
    for with_defaults in [true, false] {
        let fixture = Fixture::new();
        let mut command = init(&fixture);
        command.args([
            "--no-git",
            if with_defaults {
                "--with-defaults"
            } else {
                "--no-defaults"
            },
        ]);
        command.assert().success();
        let manifest = fixture.config();
        let targets = manifest["targets"].as_table().unwrap();
        let agy = targets["agy"].as_table().unwrap();
        assert_eq!(agy.len(), 3);
        assert_eq!(agy["enabled"].as_bool(), Some(false));
        assert_eq!(agy["scope"].as_str(), Some("user"));
        assert_eq!(agy["path"].as_str(), Some("~/.gemini/config/skills"));
        for host in ["claude", "codex", "hermes"] {
            assert_eq!(targets[host]["enabled"].as_bool(), Some(true));
        }
        assert_eq!(manifest.contains_key("skills"), with_defaults);
    }
}

#[test]
fn init_registry_commits_scaffold_when_git_repo_already_exists() {
    let fixture = Fixture::new();
    git(&fixture, &["init"]);
    let readme = fixture.registry.join("README.md");
    fs::write(&readme, "# My Example Registry\n").unwrap();
    init(&fixture)
        .arg("--no-defaults")
        .assert()
        .success()
        .stdout(predicate::str::starts_with(
            "git repository already exists; skipped git init\nCreated initial registry commit\n",
        ));
    assert!(fs::read_to_string(readme).unwrap().starts_with(&native(
        "# My Example Registry\n\nPrivate Agent Skill Registry.\n"
    )));
    assert_eq!(
        git(&fixture, &["ls-tree", "-r", "--name-only", "HEAD"]),
        ".gitignore\nREADME.md\nmy-skills.toml\n"
    );
}

#[test]
fn default_seed_skills_are_defined_once_and_available_outside_tool_repo() {
    // The executable has the wheel's allowlisted files embedded. Runtime
    // discovery of a repo or a Python package is unnecessary.
    let fixture = Fixture::new();
    let bin = assert_cmd::cargo::cargo_bin!("my-skills");
    let copied = fixture
        .root
        .join("bin")
        .join(std::path::Path::new(bin).file_name().unwrap());
    fs::copy(bin, &copied).unwrap();
    assert_with_busy_retry(
        fixture
            .process(&copied)
            .arg("init-registry")
            .arg(&fixture.registry)
            .args(["--with-defaults", "--no-git"]),
    )
    .success();
    assert_eq!(
        fs::read_dir(fixture.registry.join("skills"))
            .unwrap()
            .count(),
        3
    );
    assert!(
        fixture
            .canonical("cli-inventory")
            .join("scripts/scan_tools.py")
            .is_file()
    );
}
