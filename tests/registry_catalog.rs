mod support_h;

use std::fs;

use predicates::prelude::*;
use serde_json::json;
use support_h::{Fixture, json, plain, skill};

const CATALOG: &str = r#"
[skills.alpha]
enabled = true
hosts = ["claude", "codex"]

[skills.beta]
enabled = false
hosts = ["codex"]

[skills.gamma]
enabled = true
hosts = []
"#;

fn catalog() -> Fixture {
    let fixture = Fixture::repo();
    fixture.write_manifest(CATALOG);
    for (name, summary) in [
        ("alpha", "Alpha summary from frontmatter."),
        ("beta", "Beta summary from frontmatter."),
        ("gamma", "Gamma summary from frontmatter."),
    ] {
        skill(
            &fixture.registry.join("skills"),
            name,
            summary,
            "# Example\n",
        );
    }
    fixture
}

#[test]
fn skills_table_shows_per_host_status_columns_without_summary() {
    catalog().command().arg("skills").assert().success().stderr("").stdout(
        "SKILL  ENABLED  CLAUDE   CODEX\n-----  -------  -------  -------\nalpha  yes      missing  missing\nbeta   no       -        missing\ngamma  yes      missing  missing\n",
    );
}

#[test]
fn skills_table_includes_invalid_rows_without_failing() {
    let fixture = catalog();
    fixture.write_manifest(&format!(
        "{CATALOG}\n[skills.broken]\nenabled = true\nhosts = [\"claude\"]\n"
    ));
    fixture
        .command()
        .arg("skills")
        .assert()
        .success()
        .stderr("")
        .stdout(predicate::str::contains("broken (invalid)"))
        .stdout(predicate::str::contains("alpha").and(predicate::str::contains("gamma")));
}

#[test]
fn skills_json_filters_by_host_and_enabled_state() {
    let fixture = catalog();
    let data = json(
        fixture
            .command()
            .args(["skills", "--json", "--host", "codex", "--enabled"]),
    );
    // v1.1 adds `registry`; every v1.0 key keeps its value.
    assert_eq!(data["registry"]["source"], "env");
    assert!(data["registry"]["path"].is_string());
    let mut data = data;
    data.as_object_mut().unwrap().remove("registry");
    assert_eq!(
        data,
        json!({"skills": [
            {"name": "alpha", "enabled": true, "hosts": ["claude", "codex"], "description": "Alpha summary from frontmatter.",
             "path": std::path::Path::new("skills").join("alpha"), "status": {"codex": "MISSING"}},
            {"name": "gamma", "enabled": true, "hosts": [], "description": "Gamma summary from frontmatter.",
             "path": std::path::Path::new("skills").join("gamma"), "status": {"codex": "MISSING"}}
        ]})
    );
}

#[test]
fn skills_json_includes_invalid_row_error() {
    let fixture = catalog();
    fixture.write_manifest(&format!(
        "{CATALOG}\n[skills.broken]\nenabled = true\nhosts = [\"claude\"]\n"
    ));
    let data = json(
        fixture
            .command()
            .args(["skills", "--json", "--host", "claude", "--enabled"]),
    );
    let rows = data["skills"].as_array().unwrap();
    assert_eq!(
        rows.iter()
            .map(|row| row["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["alpha", "broken", "gamma"]
    );
    assert_eq!(rows[1]["status"], json!({"claude": "MISSING"}));
    assert!(
        rows[1]["description"]
            .as_str()
            .unwrap()
            .starts_with("invalid: ")
    );
    assert!(rows[1]["error"].as_str().unwrap().contains("SKILL.md"));
}

#[test]
fn skills_with_status_uses_selected_host_only() {
    let fixture = catalog();
    let data = json(fixture.command().args([
        "skills",
        "--json",
        "--host",
        "codex",
        "--disabled",
        "--with-status",
    ]));
    let rows = data["skills"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["name"], "beta");
    assert_eq!(rows[0]["status"], json!({"codex": "MISSING"}));
    assert_eq!(rows[0]["audit"]["status"], "ok");
    assert_eq!(rows[0]["audit"]["findings"], "0");
    assert_eq!(rows[0]["provenance"]["trust_tier"], "local-authored");
}

#[test]
fn skills_rejects_unknown_host() {
    let fixture = catalog();
    for unknown in ["ghost", "all"] {
        fixture
            .command()
            .args(["skills", "--host", unknown])
            .assert()
            .code(2)
            .stderr(format!("error: unknown host: {unknown}\n"));
    }
}

#[test]
fn skills_reports_newer_state_schema_as_usage_error() {
    let fixture = catalog();
    let state = fixture.state();
    fs::create_dir_all(state.parent().unwrap()).unwrap();
    fs::write(&state, "{\"schema_version\": 2, \"installs\": []}\n").unwrap();
    fixture.command().arg("skills").assert().code(2).stderr(
        predicate::str::contains("newer my-skills").and(predicate::str::contains(plain(&state))),
    );
}

#[test]
fn skills_enabled_and_disabled_are_mutually_exclusive() {
    catalog()
        .command()
        .args(["skills", "--enabled", "--disabled"])
        .assert()
        .code(2);
}

#[test]
fn skills_json_reports_broken_v1_link() {
    let fixture = catalog();
    let canonical = fixture.canonical("alpha");
    let host = fixture.host("claude");
    fs::create_dir_all(&host).unwrap();
    support_h::link_dir(&canonical, &host.join("alpha"));
    let state = fixture.state();
    fs::create_dir_all(state.parent().unwrap()).unwrap();
    fs::write(
        &state,
        serde_json::to_vec(&json!({
            "schema_version": 1,
            "installs": [{
                "skill": "alpha",
                "host": "claude",
                "mode": "link",
                "source": plain(&canonical),
                "destination": plain(&host.join("alpha")),
                "source_hash": "",
                "installed_hash": "",
                "installed_at": "2026-01-01T00:00:00Z"
            }]
        }))
        .unwrap(),
    )
    .unwrap();
    fs::remove_dir_all(canonical).unwrap();
    let data = json(
        fixture
            .command()
            .args(["skills", "--json", "--host", "claude", "--enabled"]),
    );
    assert_eq!(data["skills"][0]["status"], json!({"claude": "BROKEN"}));
}
