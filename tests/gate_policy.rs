//! The commit/health gate applies the manifest's `[audit]` policy
//! (handoff §6, review finding: pre-commit ignored `[audit]`).

use std::fs;

use my_skills::check::validate::gate_errors;
use my_skills::registry::manifest::{CliOverrides, load_manifest};

fn registry(audit_table: &str) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let skill = tmp.path().join("skills/pics");
    fs::create_dir_all(&skill).unwrap();
    fs::write(
        skill.join("SKILL.md"),
        "---\nname: pics\ndescription: Shows a picture.\n---\n\n![x](https://example.invalid/p.png)\n",
    )
    .unwrap();
    let hosts = tmp.path().join("hosts");
    let mut toml = format!("schema_version = 1\n{audit_table}\n");
    for host in ["claude", "codex", "agy", "custom"] {
        toml.push_str(&format!(
            "\n[targets.{host}]\nenabled = false\npath = '{}'\n",
            hosts.join(host).display()
        ));
    }
    toml.push_str("\n[skills.pics]\nenabled = true\n");
    fs::write(tmp.path().join("my-skills.toml"), toml).unwrap();
    tmp
}

fn errors_for(audit_table: &str) -> Vec<String> {
    let tmp = registry(audit_table);
    let manifest = load_manifest(tmp.path(), &CliOverrides::default()).unwrap();
    gate_errors(&manifest.skill_dir("pics"), "pics", Some(&manifest))
}

#[test]
fn default_policy_lets_a_medium_finding_through() {
    let errors = errors_for("");
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn manifest_threshold_blocks_a_medium_finding() {
    let errors = errors_for("[audit]\nthreshold = \"medium\"\n");
    assert!(
        errors
            .iter()
            .any(|e| e.contains("markdown-remote-image") && e.starts_with("audit (medium)")),
        "{errors:?}"
    );
}

#[test]
fn disabled_rule_is_respected_by_the_policy_layer() {
    let errors = errors_for(
        "[audit]\nthreshold = \"medium\"\ndisabled_rules = [\"markdown-remote-image\"]\n",
    );
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn disabled_audit_skips_the_policy_layer() {
    let errors = errors_for("[audit]\nenabled = false\nthreshold = \"medium\"\n");
    assert!(errors.is_empty(), "{errors:?}");
}
