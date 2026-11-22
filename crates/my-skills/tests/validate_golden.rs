use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use my_skills::hosts::{ALL, CLAUDE};
use my_skills::validate::{
    ValidationResult, validate_name, validate_skill, validate_skill_for_host,
};
use serde::Deserialize;

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct Diagnostics {
    errors: Vec<String>,
    warnings: Vec<String>,
}

#[test]
fn matches_python_validation_for_every_fixture_and_builtin_host() {
    let golden: BTreeMap<String, BTreeMap<String, Diagnostics>> =
        serde_json::from_str(include_str!("golden/validate/validation.json")).unwrap();
    let worktree = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fixtures = worktree.join("crates/my-skills/tests/fixtures/validate");
    let shared = worktree.join("tests/fixtures");
    let temp = tempfile::tempdir().unwrap();
    let expected_hosts: BTreeSet<_> = ALL
        .iter()
        .map(|host| host.name)
        .chain(["standard"])
        .collect();

    for (name, results) in &golden {
        let fixture = fixtures.join(name);
        let source = if fixture.is_dir() {
            fixture
        } else {
            shared.join(name)
        };
        let path = temp.path().join(name);
        copy_fixture(&source, &path);
        assert_eq!(
            results.keys().map(String::as_str).collect::<BTreeSet<_>>(),
            expected_hosts
        );

        assert_diagnostics(
            name,
            "standard",
            &path,
            validate_skill(&path),
            &results["standard"],
        );
        for host in ALL {
            assert_diagnostics(
                name,
                host.name,
                &path,
                validate_skill_for_host(&path, host),
                &results[host.name],
            );
        }
    }

    for entry in fs::read_dir(fixtures).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            assert!(
                golden.contains_key(entry.file_name().to_str().unwrap()),
                "fixture missing from golden: {:?}",
                entry.path()
            );
        }
    }
}

fn copy_fixture(source: &Path, destination: &Path) {
    fs::create_dir(destination).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let path = destination.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_fixture(&entry.path(), &path);
        } else {
            fs::copy(entry.path(), path).unwrap();
        }
    }
}

fn assert_diagnostics(
    name: &str,
    host: &str,
    path: &Path,
    result: ValidationResult,
    expected: &Diagnostics,
) {
    assert_eq!(result.skill, name);
    assert_eq!(result.path, path);
    assert_eq!(result.ok(), expected.errors.is_empty(), "{name} / {host}");
    let diagnostics = Diagnostics {
        errors: result.errors,
        warnings: result.warnings,
    };
    assert_eq!(&diagnostics, expected, "{name} / {host}");
}

#[test]
fn name_rules_match_python_regex_and_count_code_points() {
    let format_error = "name must be lowercase alphanumeric words separated by single hyphens (no leading, trailing, or consecutive hyphens)";
    for name in ["a", "a1-b2-c3", "012", "a-b\n", &"a".repeat(64)] {
        assert!(validate_name(name).is_empty(), "{name:?}");
    }
    for name in [
        "",
        "Bad",
        "-lead",
        "trail-",
        "double--hyphen",
        "a_b",
        "a\r\n",
        "a\n\n",
        "é",
    ] {
        assert_eq!(validate_name(name), [format_error], "{name:?}");
    }
    assert_eq!(validate_name(&"가".repeat(64)), [format_error]);
    assert_eq!(
        validate_name(&"가".repeat(65)),
        ["name exceeds 64 characters", format_error]
    );
}

#[test]
fn host_configuration_controls_limits_and_kept_fields() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tiny-skill");
    fs::create_dir(&path).unwrap();
    fs::write(
        path.join("SKILL.md"),
        "---\nname: tiny-skill\ndescription: 가나\ncustom: yes\nother: no\n---\n",
    )
    .unwrap();
    let host = my_skills::hosts::HostConfig {
        name: "tiny",
        description_max_chars: 1,
        frontmatter_keep_fields: &["name", "description", "custom"],
        ..CLAUDE
    };
    let result = validate_skill_for_host(&path, &host);
    assert_eq!(result.errors, ["tiny description exceeds 1 characters"]);
    assert_eq!(
        result.warnings,
        ["tiny may ignore unsupported frontmatter fields: other"]
    );
}
