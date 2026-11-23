#[allow(dead_code)]
mod support_g;

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::prelude::*;
use my_skills::drive::hash::hash_directory;
use serde_json::{Value, json};
use support_g::{Fixture, MANIFEST, skill};

const REGISTERED: &str = concat!(
    "schema_version = 1\nskills_root = \"skills\"\n",
    "[skills.example]\nenabled = true\nhosts = [\"codex\"]\n",
    "[skills.disabled]\nenabled = false\nhosts = [\"codex\"]\n"
);

fn parse(assertion: &assert_cmd::assert::Assert) -> Value {
    serde_json::from_slice(&assertion.get_output().stdout).unwrap()
}

fn output(assertion: &assert_cmd::assert::Assert) -> &str {
    std::str::from_utf8(&assertion.get_output().stdout).unwrap()
}

#[test]
fn test_doctor_reports_registry_source_version_state_and_data() {
    let fixture = Fixture::isolated();
    fixture.write("my-skills.toml", REGISTERED);
    fixture.cache(&fixture.registry);
    let result = fixture
        .command()
        .env_remove("MY_SKILLS_ROOT")
        .current_dir(fixture.base.join("outside"))
        .args(["doctor", "--no-update-check"])
        .assert()
        .success();
    let text = output(&result);

    #[cfg(unix)]
    let data = fixture.base.join("xdg-data").join("my-skills");
    #[cfg(windows)]
    let data = fixture
        .base
        .join("local-app-data")
        .join("my-skills")
        .join("data");
    for line in [
        format!("my-skills {}", env!("CARGO_PKG_VERSION")),
        "Update:  skipped".to_owned(),
        format!(
            "Registry: {} (source: cache)",
            fixture.registry.canonicalize().unwrap().display()
        ),
        "Skills:   2 registered, 1 enabled".to_owned(),
        format!(
            "State:    {}",
            fixture
                .base
                .join("xdg-state")
                .join("my-skills")
                .join("state.json")
                .display()
        ),
        format!("Data:     {}", data.display()),
        "Manifest: valid".to_owned(),
    ] {
        assert!(
            text.lines().any(|actual| actual == line),
            "missing line {line}: {text}"
        );
    }
}

#[test]
fn test_doctor_without_configured_root_is_diagnostic_success() {
    let fixture = Fixture::isolated();
    let result = fixture
        .command()
        .env_remove("MY_SKILLS_ROOT")
        .current_dir(fixture.base.join("outside"))
        .args(["doctor", "--no-update-check"])
        .assert()
        .success();
    let text = output(&result);
    assert!(text.contains(
        "Registry: not configured (run 'my-skills init-registry' or 'my-skills set-root')"
    ));
    assert!(text.contains("Manifest: not checked"));
    assert!(!fixture.base.join("xdg-config").exists());
}

#[test]
fn test_doctor_reports_invalid_env_root_as_manifest_error() {
    let fixture = Fixture::isolated();
    let result = fixture
        .command()
        .env("MY_SKILLS_ROOT", fixture.base.join("missing"))
        .args(["doctor", "--no-update-check"])
        .assert()
        .code(1);
    let text = output(&result);
    assert!(text.contains("Registry: INVALID (MY_SKILLS_ROOT="));
    assert!(text.contains("Manifest: INVALID (MY_SKILLS_ROOT="));
    let result = fixture
        .command()
        .env("MY_SKILLS_ROOT", fixture.base.join("missing"))
        .args(["doctor", "--no-update-check", "--json"])
        .assert()
        .code(1);
    let report = parse(&result);
    assert_eq!(report["manifest"]["status"], "invalid");
    assert_eq!(report["manifest"]["error"], report["registry"]["error"]);
    assert!(report["registry"]["path"].is_null());
}

#[test]
fn test_doctor_no_update_check_skips_remote_lookup() {
    let fixture = Fixture::isolated();
    fixture.cache(&fixture.registry);
    let result = fixture
        .command()
        .env_remove("MY_SKILLS_ROOT")
        .current_dir(fixture.base.join("outside"))
        .args(["doctor", "--no-update-check", "--json"])
        .assert()
        .success();
    let report = parse(&result);
    assert_eq!(report["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(report["update"], "skipped");
    assert_eq!(report["registry"]["source"], "cache");
    assert_eq!(report["manifest"]["status"], "valid");
}

#[test]
fn test_doctor_agy_detection_and_default_activation() {
    for configured in [false, true] {
        for detected in [false, true] {
            let fixture = Fixture::isolated();
            if configured {
                fixture.write(
                    "my-skills.toml",
                    &format!("{MANIFEST}[targets.agy]\nenabled = true\npath = \"~/test-agy\"\n"),
                );
            }
            if detected {
                fixture.fake_executable("agy");
            }
            let result = fixture
                .command()
                .args(["doctor", "--no-update-check"])
                .assert()
                .success();
            let line = output(&result)
                .lines()
                .find(|line| line.contains("Antigravity CLI"))
                .unwrap();
            assert!(
                line.contains(if detected { "found (agy)" } else { "not found" }),
                "{line}"
            );
            assert!(
                line.contains(if configured {
                    "enabled=True"
                } else {
                    "enabled=False"
                }),
                "{line}"
            );
            let path = if configured {
                Path::new("test-agy")
            } else {
                Path::new(".gemini/config/skills")
            };
            assert!(
                line.ends_with(&fixture.base.join("home").join(path).display().to_string()),
                "{line}"
            );
        }
    }
}

#[test]
fn test_doctor_exit_zero_and_cwd_never_changes_the_active_registry() {
    let fixture = Fixture::isolated();
    fixture
        .command()
        .env_remove("MY_SKILLS_ROOT")
        .args(["doctor", "--no-update-check"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Manifest: valid"))
        .stdout(predicates::str::contains("Hosts:"));
    assert!(!fixture.base.join("xdg-config").exists());

    let active = fixture.base.join("active");
    fs::create_dir(&active).unwrap();
    fs::write(active.join("my-skills.toml"), MANIFEST).unwrap();
    fixture.cache(&active);
    let result = fixture
        .command()
        .env_remove("MY_SKILLS_ROOT")
        .args(["doctor", "--no-update-check", "--json"])
        .assert()
        .success();
    let report = parse(&result);
    assert_eq!(report["registry"]["source"], "cwd");
    assert!(
        report["registry"]["warning"]
            .as_str()
            .unwrap()
            .contains("this directory is not the active registry")
    );
    assert_eq!(
        fs::read_to_string(fixture.base.join("xdg-config/my-skills/root"))
            .unwrap()
            .trim(),
        active.to_str().unwrap()
    );
}

#[test]
fn invalid_manifest_reports_failure_but_keeps_the_resolved_registry() {
    let fixture = Fixture::isolated();
    fixture.write("my-skills.toml", "invalid manifest = !\n");
    let result = fixture
        .command()
        .args(["doctor", "--no-update-check", "--json"])
        .assert()
        .code(1);
    let report = parse(&result);
    assert_eq!(
        report["registry"]["path"],
        json!(fixture.registry.canonicalize().unwrap())
    );
    assert_eq!(report["registry"]["source"], "env");
    assert_eq!(report["manifest"]["status"], "invalid");
    assert!(
        report["manifest"]["error"]
            .as_str()
            .unwrap()
            .contains("invalid TOML")
    );
    assert!(report["full_validation"].is_null());
}

#[test]
fn doctor_json_reports_git_default_branch_dirty_rename_links_and_hook_status() {
    let fixture = Fixture::isolated();
    fixture.init();
    fixture.write("original.txt", "content\n");
    fixture.commit();
    fixture.git(&["mv", "original.txt", "renamed file.txt"]);
    fixture.write("untracked.txt", "more content\n");
    fixture.git(&["config", "core.hooksPath", ".git/custom hooks"]);
    fixture
        .command()
        .args(["hooks", "install"])
        .assert()
        .success();
    fixture.write(".git/custom hooks/post-merge", "#!/bin/sh\n# other\n");
    fs::remove_file(fixture.registry.join(".git/custom hooks/post-checkout")).unwrap();
    fixture.fake_executable("my-skills");
    let result = fixture
        .command()
        .args(["doctor", "--no-update-check", "--json"])
        .assert()
        .success();
    let report = parse(&result);

    assert_eq!(report["registry_git"]["branch"], "main");
    assert_eq!(report["registry_git"]["default_branch"], "main");
    assert_eq!(report["registry_git"]["uncommitted"], 2);
    assert_eq!(report["registry_git"]["conflicts"], 0);
    assert!(report["registry_git"]["upstream"].is_null());
    assert_eq!(report["hooks"]["on_path"], true);
    let hook_path = PathBuf::from(report["hooks"]["path"].as_str().unwrap());
    assert_eq!(
        hook_path.canonicalize().unwrap(),
        fixture
            .registry
            .join(".git/custom hooks")
            .canonicalize()
            .unwrap()
    );
    assert_eq!(
        report["hooks"]["entries"],
        json!([
            {"name": "pre-commit", "status": "installed", "version": "v1"},
            {"name": "post-merge", "status": "foreign", "version": null},
            {"name": "post-rewrite", "status": "installed", "version": "v1"},
            {"name": "post-checkout", "status": "missing", "version": null},
        ]),
    );
    let capability = report["link_capability"]["kind"].as_str().unwrap();
    match capability {
        "symlink" | "junction" => assert!(report["link_capability"]["reason"].is_null()),
        "copy fallback" => assert!(
            !report["link_capability"]["reason"]
                .as_str()
                .unwrap()
                .is_empty()
        ),
        other => panic!("unexpected link capability {other}"),
    }
    assert!(
        report["os"]
            .as_str()
            .unwrap()
            .contains(std::env::consts::OS)
    );
    assert_eq!(report["shell"], "/bin/sh");
}

#[test]
fn doctor_warns_about_branch_divergence_rebase_merge_and_conflicts_without_failing() {
    let fixture = Fixture::isolated();
    fixture.init();
    fixture.write("topic.txt", "initial\n");
    fixture.commit();
    fixture.git(&["checkout", "-qb", "release"]);
    fixture.write("topic.txt", "upstream\n");
    fixture.commit();
    fixture.git(&["checkout", "-qb", "topic", "main"]);
    fixture.write("topic.txt", "local\n");
    fixture.commit();
    fixture.git(&["config", "remote.origin.url", "."]);
    fixture.git(&[
        "config",
        "remote.origin.fetch",
        "+refs/heads/*:refs/remotes/origin/*",
    ]);
    fixture.git(&["update-ref", "refs/remotes/origin/release", "release"]);
    fixture.git(&[
        "symbolic-ref",
        "refs/remotes/origin/HEAD",
        "refs/remotes/origin/release",
    ]);
    fixture.git(&["branch", "--set-upstream-to=origin/release"]);
    fixture
        .git_command(&["merge", "--no-edit", "release"])
        .assert()
        .code(1);
    fs::create_dir(fixture.registry.join(".git/rebase-apply")).unwrap();

    let result = fixture
        .command()
        .args(["doctor", "--no-update-check", "--json"])
        .assert()
        .success();
    let report = parse(&result);
    let git = &report["registry_git"];
    assert_eq!(git["branch"], "topic");
    assert_eq!(git["default_branch"], "release");
    assert_eq!(git["rebase_in_progress"], true);
    assert_eq!(git["merge_in_progress"], true);
    assert_eq!(git["uncommitted"], 1);
    assert_eq!(git["conflicts"], 1);
    assert_eq!(git["upstream"], "origin/release");
    assert_eq!(git["ahead"], 1);
    assert_eq!(git["behind"], 1);
    let result = fixture
        .command()
        .args(["doctor", "--no-update-check"])
        .assert()
        .success();
    let text = output(&result);
    for warning in [
        "current branch topic differs from default branch release",
        "rebase in progress",
        "merge in progress",
        "1 uncommitted changes",
        "1 unresolved conflicts",
        "ahead 1 / behind 1 (origin/release)",
    ] {
        assert!(text.contains(&format!("warning: {warning}")), "{text}");
    }
}

#[test]
fn doctor_connection_summary_and_full_validation_find_different_problems() {
    let fixture = Fixture::isolated();
    fixture.init();
    fixture.write("my-skills.toml", &format!(
        "{REGISTERED}[targets.claude]\nenabled = false\n[targets.hermes]\nenabled = false\n[targets.codex]\npath = \"~/target\"\n[skills.broken]\nhosts = [\"codex\"]\n"
    ));
    fixture.write("skills/example/SKILL.md", &skill("example"));
    fixture.write("skills/broken/SKILL.md", "pre-existing invalid content\n");
    fixture.write("skills/disabled/SKILL.md", "also invalid, but disabled\n");
    fixture.commit();

    let intended = fixture.base.join("home/target");
    let source = fixture.registry.join("skills/example");
    let copy = intended.join("example");
    fs::create_dir_all(&copy).unwrap();
    fs::copy(source.join("SKILL.md"), copy.join("SKILL.md")).unwrap();
    let hash = hash_directory(&source).unwrap();
    let state_path = fixture.base.join("xdg-state/my-skills/state.json");
    fs::create_dir_all(state_path.parent().unwrap()).unwrap();
    fs::write(state_path, json!({"schema_version": 1, "installs": [
        {
            "skill": "example", "host": "codex", "mode": "copy", "source": source,
            "destination": copy, "source_hash": hash, "installed_hash": hash,
            "installed_at": "2026-01-01T00:00:00Z", "fallback_reason": "fixture fallback",
        },
        {
            "skill": "broken", "host": "codex", "mode": "link",
            "source": fixture.registry.join("skills/broken"), "destination": intended.join("broken"),
            "source_hash": "", "installed_hash": "", "installed_at": "2026-01-01T00:00:00Z",
        },
    ]}).to_string()).unwrap();

    fixture
        .command()
        .args(["hooks", "run", "pre-commit"])
        .assert()
        .success();
    let result = fixture
        .command()
        .args(["doctor", "--no-update-check", "--json"])
        .assert()
        .success();
    let report = parse(&result);
    assert_eq!(
        report["connections"],
        json!({
            "link": 1, "copy": 1, "fallback": 1,
            // `disabled` is neither wanted nor connected, so it is not MISSING.
            "statuses": {"FRESH": 1, "MISSING": 1},
            "warnings": [],
        })
    );
    assert_eq!(report["full_validation"]["checked"], 2);
    let failures = report["full_validation"]["failures"].as_array().unwrap();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0]["skill"], "broken");
    let result = fixture
        .command()
        .args(["doctor", "--no-update-check"])
        .assert()
        .success();
    assert!(output(&result).contains("Connections: link 1 / copy 1 (fallback 1)"));
    assert!(output(&result).contains("Full validation: 2 checked, 1 failed"));
    assert!(output(&result).contains("[FAIL] broken:"));
}

#[test]
fn doctor_tolerates_invalid_connection_state_and_non_git_registries() {
    let fixture = Fixture::isolated();
    let state = fixture.base.join("xdg-state/my-skills/state.json");
    fs::create_dir_all(state.parent().unwrap()).unwrap();
    fs::write(state, "{broken").unwrap();
    let result = fixture
        .command()
        .args(["doctor", "--no-update-check", "--json"])
        .assert()
        .success();
    let report = parse(&result);
    assert_eq!(report["manifest"]["status"], "valid");
    assert!(
        report["registry_git"]["warnings"][0]
            .as_str()
            .unwrap()
            .contains("not a Git worktree")
    );
    assert!(report["hooks"].is_null());
    assert!(
        report["connections"]["warnings"][0]
            .as_str()
            .unwrap()
            .contains("invalid JSON")
    );
}
