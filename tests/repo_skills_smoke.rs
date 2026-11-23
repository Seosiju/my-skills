#[allow(dead_code)]
mod support_g;

use std::fs;
use std::path::{Path, PathBuf};

use my_skills::check::audit::{AuditPolicy, run_audit};
use my_skills::check::validate::compose_validation;
use serde_json::Value;
use support_g::Fixture;

fn repo_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn seed_skills() -> Vec<PathBuf> {
    let mut skills: Vec<_> = fs::read_dir(repo_root().join("skills"))
        .expect("read the tool repo's skills directory")
        .map(|entry| entry.expect("read a skills directory entry").path())
        .filter(|path| path.join("SKILL.md").is_file())
        .collect();
    skills.sort();
    assert!(!skills.is_empty(), "the tool repo has no seed skills");
    skills
}

#[test]
fn repo_seed_skills_pass_validation_and_audit() {
    let policy = AuditPolicy::default();
    for path in seed_skills() {
        let validation = compose_validation(&path);
        let audit = run_audit(&path, &policy)
            .unwrap_or_else(|error| panic!("audit failed for {}: {error}", path.display()));
        assert!(
            validation.ok() && !audit.blocked(),
            "{}: validation errors: {:?}; audit blocked: {}; audit findings: {:?}",
            path.display(),
            validation.errors,
            audit.blocked(),
            audit.findings
        );
    }
}

fn cli_stdout(fixture: &Fixture, args: &[&str]) -> String {
    // Use the environment to select the tool repo even from an unrelated cwd.
    let temp = fixture.base.join("outside");
    let output = fixture
        .command()
        .current_dir(&temp)
        .env("MY_SKILLS_ROOT", repo_root())
        .env("TEMP", &temp)
        .env("TMP", &temp)
        .env("TMPDIR", &temp)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_NO_LAZY_FETCH", "1")
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

#[test]
fn repo_manifest_supports_read_only_cli_commands() {
    let fixture = Fixture::isolated();
    let doctor = cli_stdout(&fixture, &["doctor", "--no-update-check"]);
    let registry = format!(
        "Registry: {} (source: env)",
        repo_root().canonicalize().unwrap().display()
    );
    for expected in [&registry, "Manifest: valid", "Update:  skipped"] {
        assert!(
            doctor.lines().any(|line| line == expected),
            "doctor output missing {expected:?}:\n{doctor}"
        );
    }

    let catalog = cli_stdout(&fixture, &["skills", "--json"]);
    let json: Value = serde_json::from_str(&catalog)
        .unwrap_or_else(|error| panic!("invalid skills JSON: {error}\n{catalog}"));
    let entries = json["skills"].as_array().expect("expected a skills array");
    for path in seed_skills() {
        let name = path.file_name().unwrap().to_str().unwrap();
        assert!(
            entries.iter().any(|entry| entry["name"] == name
                && entry["path"] == Path::new("skills").join(name).to_str().unwrap()
                && entry.get("error").is_none()),
            "seed skill {name} missing or invalid in CLI catalog:\n{catalog}"
        );
    }

    let bootstrap = cli_stdout(&fixture, &["bootstrap", "--dry-run"]);
    assert!(
        bootstrap.contains("would run: my-skills sync --yes --dry-run")
            && bootstrap.contains("then: my-skills hooks install"),
        "{bootstrap}"
    );

    for host in ["hermes", "agy"] {
        let plan = cli_stdout(
            &fixture,
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
    assert_eq!(
        fs::read_dir(fixture.base.join("home")).unwrap().count(),
        0,
        "dry runs wrote to the isolated home directory"
    );
}
