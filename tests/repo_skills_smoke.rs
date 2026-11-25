#[allow(dead_code)]
mod support_g;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use my_skills::check::validate::validate_skill;
use my_skills::platform::paths;
use serde_json::Value;
use support_g::Fixture;

fn repo_fixture() -> Fixture {
    let fixture = Fixture::isolated();
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .unwrap();
    // Copy the working contents of tracked files so local edits still get tested.
    // The allowlist excludes checkout-local manifests and untracked seed files.
    let output = fixture
        .git_command(&["ls-files", "-z", "--", "my-skills.toml", "skills/"])
        .current_dir(&repo)
        .output()
        .expect("list the tool repo's tracked manifest and seed files");
    assert!(
        output.status.success(),
        "git ls-files failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let paths = std::str::from_utf8(&output.stdout).expect("tracked seed paths should be UTF-8");
    let mut copied_manifest = false;
    for relative in paths.split_terminator('\0') {
        let source = repo.join(relative);
        assert!(
            source.symlink_metadata().unwrap().is_file()
                && source.canonicalize().unwrap().starts_with(&repo),
            "expected a regular tracked file inside the tool repo: {}",
            source.display()
        );
        let destination = fixture.registry.join(relative);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::copy(&source, &destination)
            .unwrap_or_else(|error| panic!("could not copy {}: {error}", source.display()));
        copied_manifest |= relative == "my-skills.toml";
    }
    assert!(copied_manifest, "the tool repo's manifest is not tracked");
    fixture
}

fn seed_skills(registry: &Path) -> Vec<PathBuf> {
    let mut skills: Vec<_> = fs::read_dir(registry.join("skills"))
        .expect("read the copied seed skills directory")
        .map(|entry| entry.expect("read a skills directory entry").path())
        .filter(|path| path.join("SKILL.md").is_file())
        .collect();
    skills.sort();
    assert!(!skills.is_empty(), "the tool repo has no seed skills");
    skills
}

#[test]
fn repo_seed_skills_pass_validation() {
    let fixture = repo_fixture();
    for path in seed_skills(&fixture.registry) {
        let validation = validate_skill(&path);
        assert!(
            validation.ok(),
            "{}: validation errors: {:?}",
            path.display(),
            validation.errors
        );
    }
}

fn cli_command(fixture: &Fixture) -> Command {
    let temp = fixture.base.join("outside");
    let mut command = fixture.command();
    command
        .env("APPDATA", fixture.base.join("appdata"))
        .env("TEMP", &temp)
        .env("TMP", &temp)
        .env("TMPDIR", &temp);
    command
}

fn cli_stdout(command: &mut Command, args: &[&str]) -> String {
    let output = command
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("could not run my-skills {args:?}: {error}"));
    assert!(
        output.status.success(),
        "my-skills {args:?} exited with {}; stdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("CLI stdout should be UTF-8")
}

fn assert_dir_entries(dir: &Path, expected: &[&str]) {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("could not read {}: {error}", dir.display()))
        .map(|entry| entry.unwrap().file_name())
        .collect();
    entries.sort();
    assert_eq!(entries, expected, "unexpected entries in {}", dir.display());
}

#[test]
fn repo_manifest_supports_read_only_cli_commands() {
    let fixture = repo_fixture();
    let seeds = seed_skills(&fixture.registry);
    let isolated_dirs = [
        "xdg-config",
        "xdg-state",
        "xdg-data",
        "local-app-data",
        "appdata",
    ];
    for dir in isolated_dirs {
        fs::create_dir(fixture.base.join(dir)).unwrap();
    }
    let by_env = || {
        let mut command = cli_command(&fixture);
        command.current_dir(fixture.base.join("outside"));
        command
    };

    // The unfiltered catalog contains an entry for each manifest skill.
    let catalog = cli_stdout(&mut by_env(), &["skills", "--json"]);
    let json: Value = serde_json::from_str(&catalog)
        .unwrap_or_else(|error| panic!("invalid skills JSON: {error}\n{catalog}"));
    let entries = json["skills"].as_array().expect("expected a skills array");
    assert_eq!(
        entries.len(),
        seeds.len(),
        "seed count differs from catalog:\n{catalog}"
    );
    for entry in entries {
        assert!(
            entry.get("error").is_none(),
            "invalid manifest skill:\n{entry}"
        );
    }
    for path in &seeds {
        let name = path.file_name().unwrap().to_str().unwrap();
        assert!(
            entries.iter().any(|entry| entry["name"] == name
                && entry["path"] == Path::new("skills").join(name).to_str().unwrap()),
            "seed skill {name} missing from the CLI catalog:\n{catalog}"
        );
    }

    let enabled = entries
        .iter()
        .filter(|entry| entry["enabled"] == true)
        .count();
    assert!(enabled > 0, "the tool repo has no enabled seed skills");
    let doctor = cli_stdout(&mut by_env(), &["doctor", "--no-update-check"]);
    let registry = fixture.registry.canonicalize().unwrap();
    let displayed_registry = paths::display_path(&registry);
    let env_origin = format!("Registry: {displayed_registry} (source: env)");
    let registrations = format!("Skills:   {} registered, {enabled} enabled", seeds.len());
    let validation = format!("Full validation: {enabled} checked, 0 failed");
    for expected in [
        &env_origin,
        "Manifest: valid",
        "Update:  skipped",
        &registrations,
        &validation,
    ] {
        assert!(
            doctor.lines().any(|line| line == expected),
            "doctor output missing {expected:?}:\n{doctor}"
        );
    }

    let bootstrap = cli_stdout(&mut by_env(), &["bootstrap", "--dry-run"]);
    assert!(
        bootstrap.contains("would run: my-skills sync --yes --dry-run")
            && bootstrap.contains("would suggest: run `my-skills hooks install` to check commits"),
        "{bootstrap}"
    );

    for host in ["claude", "codex", "agy"] {
        let plan = cli_stdout(
            &mut by_env(),
            &["install", "my-skills", "--host", host, "--dry-run"],
        );
        assert!(
            plan.lines().any(|line| line.split_whitespace().take(4).eq([
                "CREATE",
                "my-skills",
                "->",
                host
            ])),
            "expected an install action for {host}:\n{plan}"
        );
    }
    let config = fixture.base.join("xdg-config");
    assert_dir_entries(&config, &[]);
    let by_cwd = || {
        let mut command = cli_command(&fixture);
        command.env_remove("MY_SKILLS_ROOT");
        command
    };
    let doctor = cli_stdout(&mut by_cwd(), &["doctor", "--no-update-check"]);
    let cwd_origin = format!("Registry: {displayed_registry} (source: cwd)");
    for expected in [&cwd_origin, &validation] {
        assert!(
            doctor.lines().any(|line| line == expected),
            "doctor output missing {expected:?}:\n{doctor}"
        );
    }
    assert_dir_entries(&config, &[]);

    // First-run CWD discovery during catalog loading leaves the config empty.
    let cwd_catalog = cli_stdout(&mut by_cwd(), &["skills", "--json"]);
    let cwd_json: Value = serde_json::from_str(&cwd_catalog).unwrap();
    assert_eq!(cwd_json["skills"], json["skills"]);
    assert_eq!(cwd_json["registry"]["path"], displayed_registry);
    assert_eq!(cwd_json["registry"]["source"], "cwd");
    assert_dir_entries(&config, &[]);
    for dir in [
        "home",
        "xdg-state",
        "xdg-data",
        "local-app-data",
        "appdata",
        "outside",
    ] {
        assert_dir_entries(&fixture.base.join(dir), &[]);
    }
}
