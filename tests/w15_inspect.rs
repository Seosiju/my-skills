mod support_h;

use std::fs;

use assert_cmd::Command;
use serde_json::{Value, json};
use support_h::{Fixture, skill};

fn fixture() -> Fixture {
    let fixture = Fixture::repo();
    fixture.write_manifest(
        "[skills.alpha]\ngroup = 'first'\nhosts = ['claude']\n\
         [skills.beta]\ngroup = 'second'\n\
         [skills.gamma]\nenabled = false\ngroup = 'first'\n\
         [skills.delta]\ngroup = 'second'\nhosts = ['agy']\n",
    );
    for (name, description) in [
        ("alpha", "한글 설명"),
        ("beta", "A longer description."),
        ("gamma", "A disabled skill with a long description."),
        ("delta", "Only targets the disabled host."),
    ] {
        skill(
            &fixture.registry.join("skills"),
            name,
            description,
            "Instructions.",
        );
    }
    fixture
}

fn output(command: &mut Command, code: i32) -> Value {
    let assertion = command.assert().code(code);
    serde_json::from_slice(&assertion.get_output().stdout).unwrap()
}

#[test]
fn skills_intersects_names_with_group_host_and_enabled_filters() {
    let fixture = fixture();
    for flags in [
        &[][..],
        &["--brief"],
        &["--with-governance"],
        &["--brief", "--with-governance"],
    ] {
        let all = output(fixture.command().args(["skills", "--json"]).args(flags), 0);
        let rows = all["skills"].as_array().unwrap();
        let expected = |names: &[&str]| {
            json!({
                "registry": all["registry"],
                "skills": rows.iter().filter(|row| names.iter().any(|name| row["name"] == *name)).collect::<Vec<_>>(),
            })
        };
        for (selection, names) in [
            (&["beta"][..], &["beta"][..]),
            (&["gamma", "alpha", "alpha"], &["alpha", "gamma"]),
            (&["alpha", "gamma", "--disabled"], &["gamma"]),
            (&["beta", "--group", "first"], &[]),
            (&["--group", "first"], &["alpha", "gamma"]),
        ] {
            let actual = output(
                fixture
                    .command()
                    .args(["skills", "--json"])
                    .args(flags)
                    .args(selection),
                0,
            );
            assert_eq!(actual, expected(names), "{flags:?} {selection:?}");
        }
        let host_flags = [
            "skills",
            "--json",
            "--host",
            "claude",
            "--enabled",
            "--group",
            "first",
        ];
        let host_filtered = output(fixture.command().args(host_flags).args(flags), 0);
        let selected = output(
            fixture
                .command()
                .args(host_flags)
                .args(flags)
                .args(["alpha", "beta", "delta", "gamma"]),
            0,
        );
        assert_eq!(selected, host_filtered);
        assert_eq!(selected["skills"].as_array().unwrap().len(), 1);
        assert_eq!(selected["skills"][0]["name"], "alpha");
    }
    let error = output(fixture.command().args(["skills", "absent", "--json"]), 2);
    assert_eq!(error["error"]["code"], "unknown_skill");
}

#[test]
fn status_unions_names_and_groups_with_existing_disabled_skill_labels() {
    let fixture = fixture();
    let all = output(fixture.command().args(["status", "--json"]), 0);
    let rows = all.as_array().unwrap();
    for (selection, names) in [
        (&["alpha"][..], &["alpha"][..]),
        (&["gamma", "alpha", "alpha"], &["alpha", "gamma"]),
        (&["gamma", "--group", "second"], &["beta", "delta", "gamma"]),
        (&["--group", "first"], &["alpha", "gamma"]),
    ] {
        let actual = output(
            fixture.command().args(["status", "--json"]).args(selection),
            0,
        );
        let expected = rows
            .iter()
            .filter(|row| names.iter().any(|name| row["skill"] == *name))
            .collect::<Vec<_>>();
        assert_eq!(actual, json!(expected), "{selection:?}");
    }
    assert!(
        rows.iter()
            .any(|row| row["skill"] == "gamma" && row["status"] == "MISSING")
    );
    let error = output(fixture.command().args(["status", "absent", "--json"]), 2);
    assert_eq!(error["error"]["code"], "unknown_skill");
}

#[test]
fn validate_warns_once_for_extra_fields_and_separately_for_allowed_tools() {
    let fixture = Fixture::repo();
    let mut manifest = fs::read_to_string(fixture.manifest()).unwrap();
    manifest = manifest.replace(
        "[targets.agy]\nenabled = false",
        "[targets.agy]\nenabled = true",
    );
    manifest.push_str("\n[skills.alpha]\n");
    fs::write(fixture.manifest(), manifest).unwrap();
    fs::create_dir(fixture.canonical("alpha")).unwrap();
    fs::write(
        fixture.canonical("alpha").join("SKILL.md"),
        "---\nname: alpha\ndescription: A test skill.\nlicense: MIT\ncompatibility: Requires a terminal.\nmetadata: {version: '1'}\nallowed-tools: Read\nfoo: test\n---\n\nInstructions.\n",
    ).unwrap();
    let result = output(fixture.command().args(["validate", "--json"]), 0);
    assert_eq!(result["skills"][0]["errors"], json!([]));
    assert_eq!(
        result["skills"][0]["warnings"],
        json!([
            "'allowed-tools' has inconsistent host support and may be ignored",
            "claude, codex, agy may ignore unsupported frontmatter fields: foo",
        ])
    );
    let explicit = output(
        fixture
            .command()
            .args(["validate", "--host", "codex", "--json"]),
        0,
    );
    assert_eq!(
        explicit["skills"][0]["warnings"],
        json!([
            "'allowed-tools' has inconsistent host support and may be ignored",
            "codex may ignore unsupported frontmatter fields: foo",
        ])
    );
}

#[test]
fn doctor_reports_catalog_cost_in_characters_in_both_modes() {
    let fixture = fixture();
    for mode in ["--quick", "--no-update-check"] {
        let report = output(fixture.command().args(["doctor", mode, "--json"]), 0);
        let host = |name: &str| {
            report["hosts"]
                .as_array()
                .unwrap()
                .iter()
                .find(|host| host["name"] == name)
                .unwrap()
        };
        assert_eq!(
            host("claude")["catalog"],
            json!({
                "skills": 2, "description_chars": 26, "invalid": 0,
                "longest": [{"name": "beta", "chars": 21}, {"name": "alpha", "chars": 5}],
            })
        );
        assert_eq!(
            host("codex")["catalog"],
            json!({
                "skills": 1, "description_chars": 21, "invalid": 0,
                "longest": [{"name": "beta", "chars": 21}],
            })
        );
        assert!(host("agy").get("catalog").is_none());
        let result = fixture.command().args(["doctor", mode]).assert().success();
        let text = std::str::from_utf8(&result.get_output().stdout).unwrap();
        let line = text
            .lines()
            .find(|line| line.starts_with("Catalog: "))
            .unwrap();
        assert!(line.contains("claude 2 skills / 26 chars (longest: beta 21, alpha 5)"));
        assert!(line.contains("codex 1 skill / 21 chars (longest: beta 21)"));
        assert!(!line.contains("agy"));
    }
}

#[test]
fn doctor_counts_missing_and_malformed_metadata_without_failing() {
    let fixture = fixture();
    let mut manifest = fs::read_to_string(fixture.manifest()).unwrap();
    manifest.push_str("\n[skills.missing]\n[skills.malformed]\nhosts = ['codex']\n");
    fs::write(fixture.manifest(), manifest).unwrap();
    let malformed = fixture.canonical("malformed");
    fs::create_dir(&malformed).unwrap();
    fs::write(
        malformed.join("SKILL.md"),
        "---\ndescription: {broken\n---\n",
    )
    .unwrap();
    for mode in ["--quick", "--no-update-check"] {
        let report = output(fixture.command().args(["doctor", mode, "--json"]), 0);
        let hosts = report["hosts"].as_array().unwrap();
        for (name, skills, chars, invalid) in [("claude", 3, 26, 1), ("codex", 3, 21, 2)] {
            let catalog = &hosts.iter().find(|host| host["name"] == name).unwrap()["catalog"];
            assert_eq!(catalog["skills"], skills);
            assert_eq!(catalog["description_chars"], chars);
            assert_eq!(catalog["invalid"], invalid);
            assert_eq!(catalog["longest"][0]["name"], "beta");
        }
    }
}
