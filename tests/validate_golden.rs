//! Exact snapshots of current Rust diagnostics. The v0.4.1 Python oracle
//! was removed when the validation contract changed.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use my_skills::check::validate::{
    ValidationResult, validate_name, validate_skill, validate_skill_for_host,
};
use my_skills::drive::hosts::{ALL, CLAUDE, DescriptionLimit, SkillDisplay};
use serde::Deserialize;

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct Diagnostics {
    errors: Vec<String>,
    warnings: Vec<String>,
}

#[test]
fn matches_validation_golden_for_every_fixture_and_builtin_host() {
    let golden: BTreeMap<String, BTreeMap<String, Diagnostics>> =
        serde_json::from_str(include_str!("golden/validate/validation.json")).unwrap();
    let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let fixtures = fixture_root.join("validate");
    let shared = fixture_root.join("shared");
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
fn plain_scalars_warn_about_inline_yaml_comments() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tiny-skill");
    fs::create_dir(&path).unwrap();
    for (yaml, field, preview) in [
        (
            "name: tiny-skill\ndescription: Gets cut off # lost text",
            "description",
            "Gets cut off",
        ),
        (
            "name: tiny-skill # lost suffix\ndescription: An example",
            "name",
            "tiny-skill",
        ),
        (
            "name: tiny-skill\r\n'description': 설명 ✏️\r\n  on a second line\t# lost text",
            "description",
            "설명 ✏️ on a second line",
        ),
        (
            "{name: tiny-skill, description: Shows C# # lost text\n}",
            "description",
            "Shows C#",
        ),
        (
            "name: tiny-skill\nmetadata: {summary: &summary An example # lost text\n}\ndescription: *summary",
            "description",
            "An example",
        ),
    ] {
        fs::write(path.join("SKILL.md"), format!("---\n{yaml}\n---\n")).unwrap();
        let result = validate_skill(&path);
        assert!(result.ok(), "{result:?}");
        assert_eq!(
            result.warnings,
            [format!(
                "A YAML comment cuts off '{field}'. Agents see {preview:?}. If you want the full value to appear, put it in quotes."
            )],
            "{yaml:?}"
        );
    }
}

#[test]
fn intact_scalars_and_standalone_comments_do_not_warn() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tiny-skill");
    fs::create_dir(&path).unwrap();
    for fields in [
        "description: 'Keeps # fragment'",
        "description: \"Keeps # fragment\" # note after quoted value",
        "description: Shows C# and topic#tag",
        "description: |\n  Keeps # fragment",
        "description: > # note after block header\n  Keeps # fragment",
        "description: Keeps the value\n  # standalone comment",
        "description: #tag\n  Keeps the value",
        "description: Keeps the value\nmetadata:\n  description: Nested # comment",
        "description: Overridden # comment\ndescription: 'Keeps # fragment'",
    ] {
        fs::write(
            path.join("SKILL.md"),
            format!("---\nname: tiny-skill\n{fields}\n---\n"),
        )
        .unwrap();
        let result = validate_skill(&path);
        assert!(
            result.ok() && result.warnings.is_empty(),
            "{fields:?}: {result:?}"
        );
    }
}

#[test]
fn host_configuration_controls_limits_and_kept_fields() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tiny-skill");
    fs::create_dir(&path).unwrap();
    fs::write(
        path.join("SKILL.md"),
        "---\nname: tiny-skill\ndescription: 가나\ncustom: yes\nother: no\nlicense: MIT\ncompatibility: Terminal\nmetadata: {version: '1'}\n---\n",
    )
    .unwrap();
    let host = my_skills::drive::hosts::HostConfig {
        name: "tiny",
        display_name: "Tiny",
        skill_display: SkillDisplay {
            description_limit: Some(DescriptionLimit {
                max_chars: 1,
                suffix: "…",
                extra_field: None,
            }),
            frontmatter_keep_fields: &[
                "name",
                "description",
                "custom",
                "license",
                "compatibility",
                "metadata",
            ],
            ..CLAUDE.skill_display
        },
        ..CLAUDE
    };
    let result = validate_skill_for_host(&path, &host);
    assert!(result.ok(), "{result:?}");
    assert_eq!(
        result.warnings,
        [
            "Tiny shows a shortened description (2 characters). It keeps the first 0 characters and adds \"…\". If you want the full text to appear, keep it to 1 characters or fewer.",
            "Tiny ignores these YAML fields: other. If you want Tiny to see that information, put it in a supported field or the skill body."
        ]
    );
}
