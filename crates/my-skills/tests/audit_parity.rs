use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use my_skills::check::audit::{
    AuditFinding, AuditPolicy, AuditResult, Severity, audit_skills, cross_skill_findings,
    finding_json, format_gate, gate_json, policy_from_settings, result_json, run_audit,
};
use my_skills::registry::manifest::AuditSettings;
use serde_json::{Value, json};
use tempfile::tempdir;

const GOLDEN: &str = include_str!("golden/audit/golden.json");

fn golden() -> Value {
    serde_json::from_str(GOLDEN).unwrap()
}

fn fixture(path: &Value) -> PathBuf {
    let relative = path.as_str().unwrap().split('/').collect::<PathBuf>();
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join(relative)
}

fn as_json(findings: &[AuditFinding]) -> Value {
    json!(findings.iter().map(finding_json).collect::<Vec<_>>())
}

#[test]
fn every_fixture_matches_python_across_profiles() {
    let golden = golden();
    for case in golden["scans"].as_array().unwrap() {
        let path = fixture(&case["path"]);
        for (profile, expected) in case["profiles"].as_object().unwrap() {
            let policy = if profile == "implicit-default" {
                AuditPolicy::default()
            } else {
                policy_from_settings(&AuditSettings {
                    profile: profile.clone(),
                    ..AuditSettings::default()
                })
                .unwrap()
            };
            let result = run_audit(&path, &policy).unwrap();
            let mut expected = expected.clone();
            expected["path"] = json!(path.to_string_lossy());
            expected["findings"] = case["findings"].clone();
            expected["result_hash"] = case["result_hash"].clone();
            assert_eq!(result.root, path);
            assert_eq!(
                result_json(&result),
                expected,
                "{} ({profile})",
                path.display()
            );
        }
    }
}

#[test]
fn bundles_match_python_and_all_builtin_rules_are_exercised() {
    let golden = golden();
    let mut covered = BTreeSet::new();
    for case in golden["bundles"].as_array().unwrap() {
        let results: Vec<_> = case["paths"]
            .as_array()
            .unwrap()
            .iter()
            .map(|path| run_audit(&fixture(path), &AuditPolicy::default()).unwrap())
            .collect();
        let actual = cross_skill_findings(&results);
        assert_eq!(as_json(&actual), case["findings"], "{}", case["paths"]);
        covered.extend(actual.into_iter().map(|finding| finding.rule_id));
    }
    for scan in golden["scans"].as_array().unwrap() {
        for finding in scan["findings"].as_array().unwrap() {
            covered.insert(finding["rule_id"].as_str().unwrap().to_owned());
        }
    }
    for rule in golden["builtin_rules"].as_array().unwrap() {
        let id = rule["id"].as_str().unwrap();
        assert!(covered.contains(id), "no fixture covers {id}");
    }
    assert_eq!(
        include_str!("../src/check/audit/builtin_rules.yaml"),
        // Transitional: compares against the Python copy. Remove with the
        // Python package (handoff §7 D).
        include_str!("../../../src/my_skills/audit/builtin_rules.yaml")
    );
}

#[test]
fn gate_json_and_text_match_python_for_policy_and_skip_cases() {
    let golden = golden();
    for case in golden["gates"].as_array().unwrap() {
        let paths: Vec<_> = case["paths"]
            .as_array()
            .unwrap()
            .iter()
            .map(fixture)
            .collect();
        let config = &case["settings"];
        let policy = policy_from_settings(&AuditSettings {
            enabled: config["enabled"].as_bool().unwrap(),
            profile: config["profile"].as_str().unwrap().into(),
            threshold: config["threshold"].as_str().map(str::to_owned),
            disabled_rules: serde_json::from_value(config["disabled_rules"].clone()).unwrap(),
        })
        .unwrap();
        let gate = audit_skills(&paths, &policy, case["skip"].as_bool().unwrap()).unwrap();
        let mut expected = case["json"].clone();
        for (result, path) in expected["skills"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .zip(&paths)
        {
            result["path"] = json!(path.to_string_lossy());
        }
        assert_eq!(gate_json(&gate), expected, "{case}");
        assert_eq!(
            format_gate(&gate, case["blocked_title"].as_str().unwrap()),
            case["text"].as_str().unwrap(),
            "{case}"
        );
    }
}

#[test]
fn result_hash_matches_python_for_korean_emoji_and_control_characters() {
    let case = &golden()["unicode_hash"];
    let findings = case["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| AuditFinding {
            rule_id: value["rule_id"].as_str().unwrap().into(),
            category: value["category"].as_str().unwrap().into(),
            severity: Severity::Info,
            file: value["file"].as_str().unwrap().into(),
            message: value["message"].as_str().unwrap().into(),
            line: serde_json::from_value(value["line"].clone()).unwrap(),
        })
        .collect();
    let result = AuditResult {
        skill: "synthetic".into(),
        root: PathBuf::from("synthetic"),
        findings,
        profile: "default".into(),
        threshold: None,
    };
    assert_eq!(result.result_hash(), case["result_hash"].as_str().unwrap());
}

#[test]
fn severity_parsing_matches_python_and_disabled_audit_clears_threshold() {
    let mut settings = AuditSettings {
        profile: "unrecognized".into(),
        ..AuditSettings::default()
    };
    assert_eq!(
        policy_from_settings(&settings)
            .unwrap()
            .effective_threshold(),
        Some(Severity::Critical)
    );
    for (label, severity) in [
        ("INFO", Severity::Info),
        (" \tlow ", Severity::Low),
        ("medium", Severity::Medium),
        ("\u{1c}hıgh\u{1f}", Severity::High),
        ("cRiTiCaL", Severity::Critical),
    ] {
        settings.threshold = Some(label.into());
        assert_eq!(
            policy_from_settings(&settings)
                .unwrap()
                .effective_threshold(),
            Some(severity)
        );
    }
    settings.threshold = Some(" verbose ".into());
    assert_eq!(
        policy_from_settings(&settings).unwrap_err().to_string(),
        "unknown audit severity:  verbose "
    );
    settings.threshold = Some("info".into());
    settings.enabled = false;
    settings.disabled_rules = vec!["secret".into(), "secret".into()];
    let policy = policy_from_settings(&settings).unwrap();
    assert_eq!(policy.disabled_rules, ["secret"]);
    let path = fixture(&json!("crates/my-skills/tests/fixtures/shared/secret-skill"));
    let disabled = run_audit(&path, &policy).unwrap();
    assert!(disabled.findings.is_empty());
    assert!(!disabled.blocked());
    assert_eq!(disabled.profile, settings.profile);
    assert_eq!(disabled.threshold, None);
}

#[test]
fn scans_native_paths_in_component_order_and_skips_runtime_artifacts() {
    let skill = tempdir().unwrap();
    for dir in ["nested", ".git", ".omc", "__pycache__"] {
        fs::create_dir(skill.path().join(dir)).unwrap();
    }
    for path in [
        ".git/sample.txt",
        ".omc/sample.txt",
        "__pycache__/sample.txt",
        ".DS_Store",
    ] {
        fs::write(skill.path().join(path), "op read\0").unwrap();
    }
    let sender = Path::new("nested").join("sender.sh");
    for path in [&sender, Path::new("nested-a.sh"), Path::new("Z.sh")] {
        fs::write(skill.path().join(path), "curl https://example.invalid/").unwrap();
    }

    let result = run_audit(&skill.path().join("."), &AuditPolicy::default()).unwrap();
    assert_eq!(
        result_json(&result)["path"],
        json!(skill.path().to_string_lossy())
    );
    let mut expected = vec![sender.to_str().unwrap(), "nested-a.sh"];
    if cfg!(windows) {
        expected.push("Z.sh");
    } else {
        expected.insert(0, "Z.sh");
    }
    assert!(
        result
            .findings
            .iter()
            .all(|finding| finding.rule_id == "network-sender")
    );
    let files: Vec<_> = result
        .findings
        .iter()
        .map(|finding| finding.file.as_str())
        .collect();
    assert_eq!(files, expected);
}

#[cfg(unix)]
#[test]
fn scans_regular_file_symlinks_and_skips_linked_subdirectories() {
    use std::os::unix::fs::symlink;

    let temp = tempdir().unwrap();
    let root = temp.path().join("skill");
    let other = temp.path().join("other");
    fs::create_dir(&root).unwrap();
    fs::create_dir(&other).unwrap();
    fs::write(other.join("sample"), "op read").unwrap();
    symlink(other.join("sample"), root.join("file.txt")).unwrap();
    symlink(&other, root.join("directory")).unwrap();
    symlink(&root, temp.path().join("skill-link")).unwrap();
    let result = run_audit(&temp.path().join("skill-link"), &AuditPolicy::default()).unwrap();
    assert_eq!(result.skill, "skill-link");
    assert_eq!(result.findings.len(), 1);
    assert_eq!(result.findings[0].file, "file.txt");
    assert_eq!(result.findings[0].rule_id, "credential-reader");
}
