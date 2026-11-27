//! `skills` listing, filters, connections, `--brief`, and removed-command guidance.

mod common;

mod registry_catalog {
    use std::fs;

    use crate::common::{Fixture, json, plain, skill};
    use my_skills::platform::paths::display_path;
    use predicates::prelude::*;
    use serde_json::json;

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
    fn skills_table_labels_rows_without_a_readable_description() {
        let fixture = catalog();
        fixture.write_manifest(&format!(
        "{CATALOG}\n[skills.broken]\nenabled = true\nhosts = [\"claude\"]\n[skills.no-frontmatter]\n[skills.no-description]\n"
    ));
        for (name, content) in [
            ("no-frontmatter", "Body text."),
            (
                "no-description",
                "---\nname: no-description\n---\nBody text.",
            ),
        ] {
            fs::create_dir(fixture.canonical(name)).unwrap();
            fs::write(fixture.canonical(name).join("SKILL.md"), content).unwrap();
        }
        fixture
            .command()
            .arg("skills")
            .assert()
            .success()
            .stderr("")
            .stdout(predicate::str::contains("broken (no description)"))
            .stdout(predicate::str::contains("no-frontmatter (no description)"))
            .stdout(predicate::str::contains("no-description (no description)"))
            .stdout(predicate::str::contains("(invalid)").not())
            .stdout(predicate::str::contains("alpha").and(predicate::str::contains("gamma")));
    }

    #[test]
    fn skills_json_filters_by_host_and_enabled_state() {
        let fixture = catalog();
        let data =
            json(
                fixture
                    .command()
                    .args(["skills", "--json", "--host", "codex", "--enabled"]),
            );
        // Registry metadata and new row fields extend the existing shape.
        assert_eq!(data["registry"]["source"], "env");
        assert!(data["registry"]["path"].is_string());
        let mut data = data;
        data.as_object_mut().unwrap().remove("registry");
        assert_eq!(
            data,
            json!({"skills": [
                {"name": "alpha", "enabled": true, "hosts": ["claude", "codex"], "description": "Alpha summary from frontmatter.",
                 "path": std::path::Path::new("skills").join("alpha"), "status": {"codex": "MISSING"},
                 "canonical_path": display_path(&fixture.canonical("alpha")), "group": null,
                 "connections": {"codex": {"mode": null, "link_kind": null, "fallback_reason": null}}},
                {"name": "gamma", "enabled": true, "hosts": [], "description": "Gamma summary from frontmatter.",
                 "path": std::path::Path::new("skills").join("gamma"), "status": {"codex": "MISSING"},
                 "canonical_path": display_path(&fixture.canonical("gamma")), "group": null,
                 "connections": {"codex": {"mode": null, "link_kind": null, "fallback_reason": null}}}
            ]})
        );
    }

    #[test]
    fn skills_json_includes_metadata_read_errors_without_inventing_a_description() {
        let fixture = catalog();
        fixture.write_manifest(&format!(
            "{CATALOG}\n[skills.broken]\nenabled = true\nhosts = [\"claude\"]\n"
        ));
        let data =
            json(
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
        assert_eq!(rows[1]["description"], "");
        assert!(
            rows[1]["error"]
                .as_str()
                .unwrap()
                .starts_with("Could not read skill metadata:")
        );
        assert!(rows[1]["error"].as_str().unwrap().contains("SKILL.md"));
    }

    #[test]
    fn skills_keeps_legacy_yaml_scalar_descriptions_in_the_catalog() {
        let fixture = catalog();
        for (name, yaml) in [("alpha", "false"), ("beta", "0"), ("gamma", "0123")] {
            fs::write(
                fixture.canonical(name).join("SKILL.md"),
                format!("---\nname: {name}\ndescription: {yaml}\n---\nBody.\n"),
            )
            .unwrap();
        }
        let data = json(fixture.command().args(["skills", "--json"]));
        let rows = data["skills"].as_array().unwrap();
        for (row, (name, description)) in
            rows.iter()
                .zip([("alpha", "false"), ("beta", "0"), ("gamma", "0123")])
        {
            assert_eq!(row["name"], name);
            assert_eq!(row["description"], description);
            assert!(row.get("error").is_none(), "{row}");
        }
        assert_eq!(rows.len(), 3);
        fixture
        .command()
        .arg("skills")
        .assert()
        .success()
        .stdout("SKILL  ENABLED  CLAUDE   CODEX\n-----  -------  -------  -------\nalpha  yes      missing  missing\nbeta   no       -        missing\ngamma  yes      missing  missing\n");
    }

    #[test]
    fn skills_catalog_keeps_the_dev_1_3_1_output_for_explicit_null() {
        let fixture = catalog();
        fs::write(
            fixture.canonical("alpha").join("SKILL.md"),
            "---\nname: alpha\ndescription: null\n---\nBody.\n",
        )
        .unwrap();
        let data = json(fixture.command().args(["skills", "alpha", "--json"]));
        let row = &data["skills"][0];
        assert_eq!(row["name"], "alpha");
        assert_eq!(row["description"], "null");
        assert!(row.get("error").is_none(), "{row}");
        fixture
        .command()
        .args(["skills", "alpha"])
        .assert()
        .success()
        .stdout("SKILL  ENABLED  CLAUDE   CODEX\n-----  -------  -------  -------\nalpha  yes      missing  missing\n");
    }

    #[test]
    fn skills_json_reports_connections_and_omits_them_in_brief_without_governance() {
        let fixture = catalog();
        let alpha_canonical = fixture.canonical("alpha");
        let claude_host = fixture.host("claude");
        let codex_host = fixture.host("codex");
        fs::create_dir_all(&claude_host).unwrap();
        fs::create_dir_all(&codex_host).unwrap();
        crate::common::link_dir(&alpha_canonical, &claude_host.join("alpha"));
        let codex_copy = codex_host.join("alpha");
        fs::create_dir_all(&codex_copy).unwrap();
        fs::copy(
            alpha_canonical.join("SKILL.md"),
            codex_copy.join("SKILL.md"),
        )
        .unwrap();
        let hash = my_skills::drive::hash::hash_directory(&alpha_canonical).unwrap();

        let state = fixture.state();
        fs::create_dir_all(state.parent().unwrap()).unwrap();
        fs::write(
            &state,
            serde_json::to_vec(&json!({
                "schema_version": 1,
                "installs": [
                    {
                        "skill": "alpha",
                        "host": "claude",
                        "mode": "link",
                        "source": plain(&alpha_canonical),
                        "destination": plain(&claude_host.join("alpha")),
                        "source_hash": "",
                        "installed_hash": "",
                        "installed_at": "2026-01-01T00:00:00Z"
                    },
                    {
                        "skill": "alpha",
                        "host": "codex",
                        "mode": "copy",
                        "source": plain(&alpha_canonical),
                        "destination": plain(&codex_copy),
                        "source_hash": hash,
                        "installed_hash": hash,
                        "installed_at": "2026-01-01T00:00:00Z",
                        "fallback_reason": "fixture fallback"
                    }
                ]
            }))
            .unwrap(),
        )
        .unwrap();

        let expected_link_kind = if cfg!(windows) { "junction" } else { "symlink" };
        let data = json(fixture.command().args(["skills", "--json"]));
        let rows = data["skills"].as_array().unwrap();
        assert_eq!(rows[0]["name"], "alpha");
        assert_eq!(
            rows[0]["connections"],
            json!({
                "claude": {"mode": "link", "link_kind": expected_link_kind, "fallback_reason": null},
                "codex": {"mode": "copy", "link_kind": null, "fallback_reason": "fixture fallback"}
            })
        );
        assert_eq!(
            rows[1]["connections"],
            json!({
                "claude": {"mode": null, "link_kind": null, "fallback_reason": null},
                "codex": {"mode": null, "link_kind": null, "fallback_reason": null}
            })
        );
        for row in rows {
            assert!(row.get("governance").is_none(), "{row}");
            assert!(row.get("provenance").is_none(), "{row}");
        }

        let brief = json(fixture.command().args(["skills", "--json", "--brief"]));
        for row in brief["skills"].as_array().unwrap() {
            assert!(row.get("connections").is_none(), "{row}");
            assert!(row.get("governance").is_none(), "{row}");
            assert!(row.get("provenance").is_none(), "{row}");
        }

        let host_only =
            json(
                fixture
                    .command()
                    .args(["skills", "--json", "--host", "codex", "--disabled"]),
            );
        let rows = host_only["skills"].as_array().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["name"], "beta");
        assert_eq!(rows[0]["status"], json!({"codex": "MISSING"}));
        assert_eq!(
            rows[0]["connections"],
            json!({"codex": {"mode": null, "link_kind": null, "fallback_reason": null}})
        );

        fixture
            .command()
            .arg("skills")
            .assert()
            .success()
            .stdout(predicate::str::contains("TRUST").not());
        for removed_flag in ["--with-governance", "--with-status"] {
            fixture
                .command()
                .args(["skills", removed_flag])
                .assert()
                .code(2);
        }
    }

    #[test]
    fn removed_status_and_resolve_commands_return_guidance_exit_2() {
        let fixture = catalog();
        for (args, needle) in [
            (
                &["status"][..],
                "'status' was removed in v1.5.0; use 'my-skills skills'",
            ),
            (
                &["resolve", "alpha"][..],
                "'resolve' was removed in v1.5.0; use 'my-skills skills <name> --json' for canonical_path",
            ),
        ] {
            fixture
                .command()
                .args(args)
                .assert()
                .code(2)
                .stderr(predicate::str::contains(needle));
            let mut json_cmd = fixture.command();
            json_cmd.args(args).arg("--json");
            let assert = json_cmd.assert().code(2);
            let payload: serde_json::Value =
                serde_json::from_slice(&assert.get_output().stdout).unwrap();
            assert_eq!(payload["error"]["code"], "usage");
            assert!(
                payload["error"]["message"]
                    .as_str()
                    .unwrap()
                    .contains(needle),
                "{payload}"
            );
        }
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
            predicate::str::contains("newer my-skills")
                .and(predicate::str::contains(plain(&state))),
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
        crate::common::link_dir(&canonical, &host.join("alpha"));
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
        let data =
            json(
                fixture
                    .command()
                    .args(["skills", "--json", "--host", "claude", "--enabled"]),
            );
        assert_eq!(data["skills"][0]["status"], json!({"claude": "BROKEN"}));
    }
}

mod w15_inspect {
    use std::fs;

    use crate::common::json_code;
    use crate::common::{Fixture, skill};
    use serde_json::json;

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

    #[test]
    fn skills_intersects_names_with_group_host_and_enabled_filters() {
        let fixture = fixture();
        for flags in [&[][..], &["--brief"]] {
            let all = json_code(fixture.command().args(["skills", "--json"]).args(flags), 0);
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
                let actual = json_code(
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
            let host_filtered = json_code(fixture.command().args(host_flags).args(flags), 0);
            let selected = json_code(
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
        let error = json_code(fixture.command().args(["skills", "absent", "--json"]), 2);
        assert_eq!(error["error"]["code"], "unknown_skill");
    }

    #[test]
    fn validate_reports_ignored_fields_for_each_target_host() {
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
        let result = json_code(fixture.command().args(["validate", "--json"]), 0);
        assert_eq!(result["skills"][0]["errors"], json!([]));
        assert_eq!(
            result["skills"][0]["warnings"],
            json!([
                "Claude Code ignores these YAML fields: foo. If you want Claude Code to see that information, put it in a supported field or the skill body.",
                "Codex ignores these YAML fields: allowed-tools, foo. If you want Codex to see that information, put it in a supported field or the skill body.",
                "Antigravity CLI ignores these YAML fields: allowed-tools, foo. If you want Antigravity CLI to see that information, put it in a supported field or the skill body.",
            ])
        );
        let explicit = json_code(
            fixture
                .command()
                .args(["validate", "--host", "codex", "--json"]),
            0,
        );
        assert_eq!(
            explicit["skills"][0]["warnings"],
            json!([
                "Codex ignores these YAML fields: allowed-tools, foo. If you want Codex to see that information, put it in a supported field or the skill body.",
            ])
        );
    }

    #[test]
    fn doctor_reports_catalog_cost_in_characters_in_both_modes() {
        let fixture = fixture();
        for mode in ["--quick", "--no-update-check"] {
            let report = json_code(fixture.command().args(["doctor", mode, "--json"]), 0);
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
        manifest.push_str(
            "\n[skills.missing]\n[skills.malformed]\nhosts = ['codex']\n[skills.no-description]\n",
        );
        fs::write(fixture.manifest(), manifest).unwrap();
        let malformed = fixture.canonical("malformed");
        fs::create_dir(&malformed).unwrap();
        fs::write(
            malformed.join("SKILL.md"),
            "---\ndescription: {broken\n---\n",
        )
        .unwrap();
        fs::create_dir(fixture.canonical("no-description")).unwrap();
        fs::write(
            fixture.canonical("no-description").join("SKILL.md"),
            "---\nname: no-description\n---\nBody.\n",
        )
        .unwrap();
        for mode in ["--quick", "--no-update-check"] {
            let report = json_code(fixture.command().args(["doctor", mode, "--json"]), 0);
            let hosts = report["hosts"].as_array().unwrap();
            for (name, skills, chars, invalid) in [("claude", 4, 26, 2), ("codex", 4, 21, 3)] {
                let catalog = &hosts.iter().find(|host| host["name"] == name).unwrap()["catalog"];
                assert_eq!(catalog["skills"], skills);
                assert_eq!(catalog["description_chars"], chars);
                assert_eq!(catalog["invalid"], invalid);
                assert_eq!(catalog["longest"][0]["name"], "beta");
            }
            let result = fixture.command().args(["doctor", mode]).assert().success();
            let text = std::str::from_utf8(&result.get_output().stdout).unwrap();
            let catalog = text
                .lines()
                .find(|line| line.starts_with("Catalog: "))
                .unwrap();
            assert!(catalog.contains(
                "claude 4 skills / 26 chars (longest: beta 21, alpha 5) (2 without a description)"
            ));
            assert!(catalog.contains(
                "codex 4 skills / 21 chars (longest: beta 21) (3 without a description)"
            ));
            assert!(!catalog.contains("invalid"));
        }
    }
}
