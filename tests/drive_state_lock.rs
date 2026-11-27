//! Manifest/state/data loading, content hashes, locks, and the root cache.

mod common;

mod manifest_state_data {
    use std::collections::BTreeMap;
    use std::path::Path;
    use std::process::Command;
    use std::{env, fs};

    use my_skills::Error;
    use my_skills::drive::hosts;
    use my_skills::drive::link::LinkKind;
    use my_skills::drive::state::{State, default_state_path};
    use my_skills::platform::data::skill_data_path;
    use my_skills::registry::manifest::{
        CliOverrides, Defaults, InstallMode, LOCAL_MANIFEST_FILE, MANIFEST_FILE, expand_path,
        load_manifest, selected_skills,
    };
    use serde_json::{Value, json};
    use tempfile::tempdir;

    const BASE: &str = include_str!("fixtures/manifest/base.toml");
    const LOCAL: &str = include_str!("fixtures/manifest/local.toml");
    const LEGACY_STATE: &[u8] = include_bytes!("fixtures/state/state.json");

    // Pass process-wide settings to a child so parallel tests never change each other's environment.
    fn with_temp_home(test: &str, run: impl FnOnce(&Path)) {
        if let Some(root) = env::var_os("MY_SKILLS_STREAM_B_TEST_ROOT") {
            run(Path::new(&root));
            return;
        }

        // The test binary names tests by their path below the crate root.
        let (_, module) = module_path!().split_once("::").unwrap();
        let name = format!("{module}::{test}");
        let temp = tempdir().unwrap();
        let root = temp.path();
        let output = Command::new(env::current_exe().unwrap())
            .args(["--exact", &name, "--include-ignored", "--nocapture"])
            .current_dir(root)
            .env("MY_SKILLS_STREAM_B_TEST_ROOT", root)
            .env("HOME", root.join("home"))
            .env("USERPROFILE", root.join("home"))
            .env("APPDATA", root.join("config"))
            .env("XDG_STATE_HOME", root.join("state"))
            .env("XDG_DATA_HOME", root.join("data"))
            .env("XDG_CONFIG_HOME", root.join("config"))
            .env("LOCALAPPDATA", root.join("appdata"))
            .env("MY_SKILLS_STREAM_B_DIR", "expanded")
            .env("MY_SKILLS_STREAM_B_TILDE", "~")
            .env("MY_SKILLS_STREAM_B_REFERENCE", "$MY_SKILLS_STREAM_B_DIR")
            .env_remove("MY_SKILLS_STREAM_B_UNKNOWN")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success() && stdout.contains("1 passed"),
            "{stdout}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn manifest_merges_main_local_and_cli_by_key() {
        with_temp_home("manifest_merges_main_local_and_cli_by_key", |root| {
            fs::write(root.join(MANIFEST_FILE), BASE).unwrap();
            let base = load_manifest(root, &CliOverrides::default()).unwrap();
            assert_eq!(base.skills_root, "base-skills");
            assert_eq!(base.defaults.install_mode, InstallMode::Copy);
            assert_eq!(base.targets["claude"].path, root.join("home/base-target"));
            assert!(base.targets["codex"].enabled);

            fs::write(root.join(LOCAL_MANIFEST_FILE), LOCAL).unwrap();
            let local = load_manifest(root, &CliOverrides::default()).unwrap();
            assert_eq!(local.root, root);
            assert_eq!(local.schema_version, 2);
            assert_eq!(local.skills_dir(), root.join("local-skills"));
            assert_eq!(
                local.defaults,
                Defaults {
                    install_mode: InstallMode::Link,
                    collision: "replace".into(),
                    verify_after_install: false,
                }
            );
            assert_eq!(local.targets["claude"].path, root.join("home/local-target"));
            assert_eq!(
                local.targets["local-only"].path,
                root.join("home/another-target")
            );
            assert_eq!(
                local.targets["custom"].path,
                root.join("home/expanded/custom")
            );
            assert_eq!(local.targets["custom"].scope, "user");
            assert!(local.targets["custom"].enabled);
            assert!(!local.targets["agy"].enabled);
            assert!(local.targets["codex"].enabled);
            assert_eq!(
                local.skills.keys().map(String::as_str).collect::<Vec<_>>(),
                ["alpha", "beta", "zeta"]
            );
            let alpha = &local.skills["alpha"];
            assert!(alpha.enabled);
            assert_eq!(alpha.hosts, ["codex", "agy"]);
            assert_eq!(alpha.source_type, "synthetic");
            assert_eq!(alpha.source_revision, "local-revision");
            assert_eq!(alpha.group.as_deref(), Some("sample-group"));
            assert_eq!(local.skills["beta"].group.as_deref(), Some("local-group"));

            let cli = CliOverrides {
                install_mode: Some(InstallMode::Copy),
                skills_root: Some("cli-skills".into()),
                target_paths: BTreeMap::from([
                    ("claude".into(), "~/cli-target".into()),
                    ("cli-only".into(), "cli-only-target".into()),
                ]),
            };
            let manifest = load_manifest(root, &cli).unwrap();
            assert_eq!(manifest.defaults.install_mode, InstallMode::Copy);
            assert_eq!(manifest.skill_dir("alpha"), root.join("cli-skills/alpha"));
            let claude = &manifest.targets["claude"];
            assert_eq!(claude.name, "claude");
            assert_eq!(claude.path, root.join("home/cli-target"));
            assert_eq!(claude.scope, "project");
            assert!(!claude.enabled);
            let cli_only = &manifest.targets["cli-only"];
            assert_eq!(
                cli_only.path,
                env::current_dir().unwrap().join("cli-only-target")
            );
            assert!(cli_only.enabled);
            assert_eq!(
                manifest
                    .targets
                    .keys()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
                ["agy", "claude", "cli-only", "codex", "custom", "local-only"]
            );
        });
    }

    #[test]
    fn manifest_uses_lenient_defaults_for_wrong_types() {
        with_temp_home("manifest_uses_lenient_defaults_for_wrong_types", |root| {
            let wrong_types = r#"
            schema_version = "not-an-int"
            skills_root = 3
            [defaults]
            collision = false
            verify_after_install = "false"
            [audit]
            enabled = 0
            profile = ["broad"]
            threshold = false
            disabled_rules = "one-rule"
            [targets.claude]
            enabled = "false"
            scope = []
            path = 3
            [targets.agy]
            enabled = 1
            path = ""
            [skills.sample]
            enabled = "false"
            hosts = 1
            source_type = true
            source_revision = 7
            group = false
        "#;
            fs::write(root.join(MANIFEST_FILE), wrong_types).unwrap();
            let manifest = load_manifest(root, &CliOverrides::default()).unwrap();
            assert_eq!(manifest.schema_version, 1);
            assert_eq!(manifest.skills_root, "skills");
            assert_eq!(manifest.defaults, Defaults::default());
            let claude = &manifest.targets["claude"];
            assert_eq!(claude.path, root.join("home/.claude/skills"));
            assert!(claude.enabled);
            assert_eq!(claude.scope, "user");
            let agy = &manifest.targets["agy"];
            assert_eq!(agy.path, root.join("home/.gemini/config/skills"));
            assert!(!agy.enabled);
            let skill = &manifest.skills["sample"];
            assert!(skill.enabled);
            assert!(skill.hosts.is_empty());
            assert!(skill.source_type.is_empty());
            assert!(skill.source_revision.is_empty());
            assert_eq!(skill.group, None);

            fs::write(root.join(MANIFEST_FILE), BASE).unwrap();
            fs::write(
                root.join(LOCAL_MANIFEST_FILE),
                r#"
            skills_root = ""
            [defaults]
            collision = 1
            [audit]
            threshold = ""
            targets = false
        "#,
            )
            .unwrap();
            let override_values = load_manifest(
                root,
                &CliOverrides {
                    skills_root: Some(String::new()),
                    ..CliOverrides::default()
                },
            )
            .unwrap();
            assert_eq!(override_values.skills_root, "base-skills");
            assert_eq!(override_values.defaults.collision, "error");

            fs::write(
                root.join(LOCAL_MANIFEST_FILE),
                "defaults = false\naudit = 7\ntargets = []\nskills = false\n",
            )
            .unwrap();
            let whole_tables = load_manifest(root, &CliOverrides::default()).unwrap();
            assert_eq!(whole_tables.defaults.collision, "skip");
            assert_eq!(
                whole_tables.targets["claude"].path,
                root.join("home/base-target")
            );
            assert!(!whole_tables.skills["alpha"].enabled);
        });
    }

    #[test]
    fn manifest_reports_missing_and_invalid_files() {
        with_temp_home("manifest_reports_missing_and_invalid_files", |root| {
            let main = root.join(MANIFEST_FILE);
            let local = root.join(LOCAL_MANIFEST_FILE);
            let cli = CliOverrides::default();
            let error = load_manifest(root, &cli).unwrap_err();
            assert!(matches!(error, Error::Manifest(_)));
            assert_eq!(
                error.to_string(),
                format!("manifest not found: {}", main.display())
            );

            fs::write(&main, "[missing-bracket").unwrap();
            let error = load_manifest(root, &cli).unwrap_err();
            assert!(matches!(error, Error::Manifest(_)));
            assert!(
                error
                    .to_string()
                    .starts_with(&format!("invalid TOML in {}:", main.display()))
            );

            fs::write(&main, "").unwrap();
            fs::write(&local, "[missing-bracket").unwrap();
            let error = load_manifest(root, &cli).unwrap_err();
            assert!(matches!(error, Error::Manifest(_)));
            assert!(
                error
                    .to_string()
                    .starts_with(&format!("invalid TOML in {}:", local.display()))
            );
        });
    }

    #[test]
    fn manifest_rejects_invalid_effective_install_modes() {
        with_temp_home("manifest_rejects_invalid_effective_install_modes", |root| {
            let main = root.join(MANIFEST_FILE);
            let local = root.join(LOCAL_MANIFEST_FILE);
            fs::write(&main, "").unwrap();
            let cli = CliOverrides::default();
            assert_eq!(
                load_manifest(root, &cli).unwrap().defaults.install_mode,
                InstallMode::Link
            );

            for (path, value) in [(&main, "\"symlink\""), (&main, "\"\""), (&local, "7")] {
                fs::write(path, format!("[defaults]\ninstall_mode = {value}\n")).unwrap();
                let error = load_manifest(root, &cli).unwrap_err();
                assert!(matches!(error, Error::Manifest(_)));
                assert_eq!(
                    error.to_string(),
                    format!(
                        "invalid install_mode in {}: {value}; expected \"link\" or \"copy\"",
                        path.display()
                    )
                );
            }
            fs::write(&local, "[defaults]\ninstall_mode = 'copy'\n").unwrap();
            assert_eq!(
                load_manifest(root, &cli).unwrap().defaults.install_mode,
                InstallMode::Copy
            );
            fs::write(&local, "[defaults]\ninstall_mode = false\n").unwrap();
            let cli = CliOverrides {
                install_mode: Some(InstallMode::Link),
                ..cli
            };
            assert_eq!(
                load_manifest(root, &cli).unwrap().defaults.install_mode,
                InstallMode::Link
            );
        });
    }

    #[test]
    fn manifest_parses_overlays_and_validates_target_install_mode() {
        with_temp_home(
            "manifest_parses_overlays_and_validates_target_install_mode",
            |root| {
                let main = root.join(MANIFEST_FILE);
                let local = root.join(LOCAL_MANIFEST_FILE);
                fs::write(
                    &main,
                    concat!(
                        "[defaults]\ninstall_mode = \"link\"\n",
                        "[targets.agy]\nenabled = true\ninstall_mode = \"copy\"\n",
                        "[targets.claude]\nenabled = true\ninstall_mode = \"copy\"\n",
                        "[targets.codex]\nenabled = true\n",
                    ),
                )
                .unwrap();
                let cli = CliOverrides::default();
                let manifest = load_manifest(root, &cli).unwrap();
                assert_eq!(
                    manifest.targets["agy"].install_mode,
                    Some(InstallMode::Copy)
                );
                assert_eq!(
                    manifest.targets["claude"].install_mode,
                    Some(InstallMode::Copy)
                );
                assert_eq!(manifest.targets["codex"].install_mode, None);
                assert_eq!(manifest.effective_install_mode("agy"), InstallMode::Copy);
                assert_eq!(manifest.effective_install_mode("claude"), InstallMode::Copy);
                assert_eq!(manifest.effective_install_mode("codex"), InstallMode::Link);

                // Overlay wins for targets that specify install_mode, and preserves main for others.
                fs::write(
                    &local,
                    concat!(
                        "[targets.claude]\ninstall_mode = \"link\"\n",
                        "[targets.agy]\nenabled = true\n",
                        "[targets.codex]\ninstall_mode = \"copy\"\n",
                    ),
                )
                .unwrap();
                let overlay = load_manifest(root, &cli).unwrap();
                assert_eq!(overlay.targets["agy"].install_mode, Some(InstallMode::Copy));
                assert_eq!(
                    overlay.targets["claude"].install_mode,
                    Some(InstallMode::Link)
                );
                assert_eq!(
                    overlay.targets["codex"].install_mode,
                    Some(InstallMode::Copy)
                );
                assert_eq!(overlay.effective_install_mode("agy"), InstallMode::Copy);
                assert_eq!(overlay.effective_install_mode("claude"), InstallMode::Link);
                assert_eq!(overlay.effective_install_mode("codex"), InstallMode::Copy);

                // CLI --mode overrides both target and default modes.
                let cli_link = CliOverrides {
                    install_mode: Some(InstallMode::Link),
                    ..CliOverrides::default()
                };
                let with_cli = load_manifest(root, &cli_link).unwrap();
                assert_eq!(with_cli.effective_install_mode("agy"), InstallMode::Link);
                assert_eq!(with_cli.effective_install_mode("codex"), InstallMode::Link);

                // Invalid values fail validation in both main and local manifests.
                fs::remove_file(&local).unwrap();
                for (path, value) in [(&main, "\"symlink\""), (&main, "\"\""), (&local, "7")] {
                    fs::write(path, format!("[targets.agy]\ninstall_mode = {value}\n")).unwrap();
                    let error = load_manifest(root, &cli).unwrap_err();
                    assert!(matches!(error, Error::Manifest(_)));
                    assert_eq!(
                        error.to_string(),
                        format!(
                            "invalid install_mode for target 'agy' in {}: {value}; expected \"link\" or \"copy\"",
                            path.display()
                        )
                    );
                }

                // A valid local overlay overrides an invalid main target install_mode.
                fs::write(&main, "[targets.agy]\ninstall_mode = \"bad\"\n").unwrap();
                fs::write(&local, "[targets.agy]\ninstall_mode = \"copy\"\n").unwrap();
                let recovered = load_manifest(root, &cli).unwrap();
                assert_eq!(
                    recovered.targets["agy"].install_mode,
                    Some(InstallMode::Copy)
                );
            },
        );
    }

    #[test]
    fn manifest_requires_a_path_for_unknown_targets() {
        with_temp_home("manifest_requires_a_path_for_unknown_targets", |root| {
            let main = root.join(MANIFEST_FILE);
            for value in ["", "enabled = true", "path = ''", "path = false"] {
                fs::write(&main, format!("[targets.unknown]\n{value}\n")).unwrap();
                assert_eq!(
                    load_manifest(root, &CliOverrides::default())
                        .unwrap_err()
                        .to_string(),
                    "target 'unknown' has no path and no built-in default"
                );
            }
            let cli = CliOverrides {
                target_paths: BTreeMap::from([("unknown".into(), "~/target".into())]),
                ..CliOverrides::default()
            };
            let manifest = load_manifest(root, &cli).unwrap();
            assert_eq!(manifest.targets["unknown"].path, root.join("home/target"));
            assert!(manifest.targets["unknown"].enabled);
        });
    }

    #[test]
    fn manifest_drops_disabled_pathless_targets_and_validates_legacy_skills() {
        with_temp_home(
            "manifest_drops_disabled_pathless_targets_and_validates_legacy_skills",
            |root| {
                fs::write(
                    root.join(MANIFEST_FILE),
                    "[skills.alpha]\nhosts = ['hermes', 'codex']\n",
                )
                .unwrap();
                fs::write(
                    root.join(LOCAL_MANIFEST_FILE),
                    "[targets.hermes]\nenabled = false\n[targets.custom]\nenabled = false\n",
                )
                .unwrap();
                let skill_dir = root.join("skills/alpha");
                fs::create_dir_all(&skill_dir).unwrap();
                fs::write(
                    skill_dir.join("SKILL.md"),
                    "---\nname: alpha\ndescription: Demo skill alpha.\n---\nBody.\n",
                )
                .unwrap();

                let manifest = load_manifest(root, &CliOverrides::default()).unwrap();
                assert_eq!(
                    manifest
                        .targets
                        .keys()
                        .map(String::as_str)
                        .collect::<Vec<_>>(),
                    ["agy", "claude", "codex"]
                );
                assert_eq!(manifest.skills["alpha"].hosts, ["hermes", "codex"]);

                let output = Command::new(assert_cmd::cargo::cargo_bin!("my-skills"))
                    .args(["validate", "alpha"])
                    .env("MY_SKILLS_ROOT", root)
                    .output()
                    .unwrap();
                assert!(output.status.success(), "{output:?}");
                assert!(String::from_utf8_lossy(&output.stdout).contains("[OK] alpha"));
            },
        );
    }

    #[test]
    fn manifest_keeps_hermes_with_an_explicit_path_as_a_custom_target() {
        with_temp_home(
            "manifest_keeps_hermes_with_an_explicit_path_as_a_custom_target",
            |root| {
                fs::write(
                    root.join(MANIFEST_FILE),
                    "[targets.hermes]\npath = '~/legacy-target'\n",
                )
                .unwrap();
                assert!(hosts::get_host("hermes").is_none());
                let cli = CliOverrides::default();
                let manifest = load_manifest(root, &cli).unwrap();
                assert!(manifest.targets["hermes"].enabled);
                assert_eq!(
                    manifest.targets["hermes"].path,
                    root.join("home/legacy-target")
                );

                fs::write(
                    root.join(LOCAL_MANIFEST_FILE),
                    "[targets.hermes]\nenabled = false\n",
                )
                .unwrap();
                let manifest = load_manifest(root, &cli).unwrap();
                assert!(!manifest.targets["hermes"].enabled);
                assert_eq!(
                    manifest.targets["hermes"].path,
                    root.join("home/legacy-target")
                );
            },
        );
    }

    #[test]
    fn manifest_selects_skills_in_sorted_manifest_order() {
        with_temp_home("manifest_selects_skills_in_sorted_manifest_order", |root| {
            fs::write(root.join(MANIFEST_FILE), BASE).unwrap();
            fs::write(root.join(LOCAL_MANIFEST_FILE), LOCAL).unwrap();
            let manifest = load_manifest(root, &CliOverrides::default()).unwrap();
            let names = |explicit: &[String], all| {
                selected_skills(&manifest, explicit, all)
                    .unwrap()
                    .into_iter()
                    .map(|skill| skill.name.as_str())
                    .collect::<Vec<_>>()
            };
            assert_eq!(names(&[], false), ["alpha", "beta"]);
            assert_eq!(names(&[], true), ["alpha", "beta", "zeta"]);
            assert_eq!(
                names(&["zeta".into(), "alpha".into()], true),
                ["alpha", "zeta"]
            );
            let error = selected_skills(
                &manifest,
                &["zulu".into(), "alpha".into(), "aardvark".into()],
                false,
            )
            .unwrap_err();
            assert_eq!(error.to_string(), "unknown skill(s): aardvark, zulu");
        });
    }

    #[test]
    fn manifest_expands_home_and_environment_variables() {
        with_temp_home("manifest_expands_home_and_environment_variables", |root| {
            let cwd = env::current_dir().unwrap();
            assert_eq!(expand_path("~"), root.join("home"));
            assert_eq!(expand_path("~//nested/path"), root.join("home/nested/path"));
            assert_eq!(
                expand_path("$MY_SKILLS_STREAM_B_TILDE/$MY_SKILLS_STREAM_B_DIR"),
                root.join("home/expanded")
            );
            assert_eq!(expand_path("${HOME}/x"), root.join("home/x"));
            assert_eq!(expand_path(""), cwd);
            assert_eq!(
                expand_path("relative/${MY_SKILLS_STREAM_B_DIR}"),
                cwd.join("relative/expanded")
            );
            for value in [
                "${MY_SKILLS_STREAM_B_UNKNOWN}/x",
                "$MY_SKILLS_STREAM_B_UNKNOWN/x",
                "${missing",
                "${}",
            ] {
                assert_eq!(expand_path(value), cwd.join(value));
            }
            assert_eq!(
                expand_path("$MY_SKILLS_STREAM_B_REFERENCE"),
                cwd.join("$MY_SKILLS_STREAM_B_DIR")
            );
            #[cfg(windows)]
            {
                assert_eq!(
                    expand_path(r"~\%MY_SKILLS_STREAM_B_DIR%"),
                    root.join("home/expanded")
                );
                assert_eq!(
                    expand_path("%MY_SKILLS_STREAM_B_UNKNOWN%"),
                    cwd.join("%MY_SKILLS_STREAM_B_UNKNOWN%")
                );
                assert_eq!(
                    expand_path("'$MY_SKILLS_STREAM_B_DIR'/%%/$$"),
                    cwd.join("'$MY_SKILLS_STREAM_B_DIR'/%/$")
                );
            }
            #[cfg(not(windows))]
            assert_eq!(
                expand_path("%MY_SKILLS_STREAM_B_DIR%"),
                cwd.join("%MY_SKILLS_STREAM_B_DIR%")
            );

            #[cfg(unix)]
            {
                fs::create_dir_all(root.join("home/actual")).unwrap();
                std::os::unix::fs::symlink("actual", root.join("home/alias")).unwrap();
                assert_eq!(
                    expand_path("~/alias/../future"),
                    root.join("home/alias/../future")
                );
            }
        });
    }

    #[test]
    fn state_uses_default_path_and_loads_a_missing_file_as_empty() {
        with_temp_home(
            "state_uses_default_path_and_loads_a_missing_file_as_empty",
            |root| {
                let path = root.join("state/my-skills/state.json");
                assert_eq!(default_state_path(), path);
                let state = State::load(None).unwrap();
                assert_eq!(state.path, path);
                assert!(state.installs.is_empty());
                assert!(!path.parent().unwrap().exists());

                state.save().unwrap();
                assert_eq!(
                    fs::read_to_string(&path).unwrap(),
                    "{\n  \"schema_version\": 1,\n  \"installs\": []\n}\n"
                );
                assert_eq!(State::load(None).unwrap(), state);
            },
        );
    }

    #[test]
    fn state_loads_v041_fixture_and_discards_audit_fields_on_save() {
        let root = tempdir().unwrap();
        let input_path = root.path().join("input.json");
        fs::write(&input_path, LEGACY_STATE).unwrap();
        let input = State::load(Some(&input_path)).unwrap();
        let output_path = root.path().join("nested/state.json");
        let mut output = State::load(Some(&output_path)).unwrap();
        for record in input.installs.values().rev() {
            output.put(record.clone());
        }
        assert_eq!(
            output.get("alpha", "claude").unwrap().mode,
            InstallMode::Link
        );
        assert_eq!(
            output.get("alpha", "codex").unwrap().mode,
            InstallMode::Copy
        );
        assert!(
            output
                .records()
                .all(|record| record.link_kind.is_none() && record.fallback_reason.is_none())
        );
        output.save().unwrap();
        let legacy = std::str::from_utf8(LEGACY_STATE).unwrap();
        // These four fields follow the last retained field in every fixture record.
        let audit_fields = concat!(
            ",\n",
            "      \"last_audit_at\": \"2026-01-02T03:04:06Z\",\n",
            "      \"last_audit_result_hash\": \"sha256:cccc\",\n",
            "      \"audit_profile\": \"fixture\",\n",
            "      \"audit_threshold\": \"high\"",
        );
        assert_eq!(legacy.matches(audit_fields).count(), 3);
        let expected = legacy.replace(audit_fields, "");
        assert_eq!(fs::read(&output_path).unwrap(), expected.as_bytes());
        assert_eq!(State::load(Some(&output_path)).unwrap(), output);
    }

    #[test]
    fn state_ignores_unknown_fields_defaults_legacy_metadata_and_keeps_new_fields() {
        let root = tempdir().unwrap();
        let path = root.path().join("state.json");
        fs::write(
            &path,
            br#"{"unrecognized":true,"installs":[{
        "skill":"alpha","host":"claude","mode":"link","source":"/fixture/src",
        "destination":"/fixture/dst","source_hash":"sha256:aaaa","installed_hash":"sha256:bbbb",
        "installed_at":"2026-01-02T03:04:05Z","link_kind":"junction","future":{"x":1}
    }]}"#,
        )
        .unwrap();
        let mut state = State::load(Some(&path)).unwrap();
        let link = state.get("alpha", "claude").unwrap().clone();
        assert_eq!(link.link_kind, Some(LinkKind::Junction));
        assert_eq!(link.fallback_reason, None);
        assert_eq!(link.source_type, "canonical");
        assert!(link.source_url.is_empty() && link.source_revision.is_empty());

        let mut copy = link;
        copy.host = "codex".into();
        copy.mode = InstallMode::Copy;
        copy.link_kind = None;
        copy.fallback_reason = Some("fixture fallback".into());
        state.put(copy);
        state.save().unwrap();
        let contents: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert!(contents.get("unrecognized").is_none());
        let records = contents["installs"].as_array().unwrap();
        assert!(records[0].get("future").is_none());
        assert!(records[0].get("fallback_reason").is_none());
        assert_eq!(records[0]["link_kind"], "junction");
        assert!(records[1].get("link_kind").is_none());
        assert_eq!(records[1]["fallback_reason"], "fixture fallback");
        assert_eq!(State::load(Some(&path)).unwrap(), state);
        assert!(!path.with_extension("json.tmp").exists());
    }

    #[test]
    fn state_reports_legacy_errors_with_recovery_instructions() {
        let root = tempdir().unwrap();
        let path = root.path().join("state.json");
        let fixture: Value = serde_json::from_slice(LEGACY_STATE).unwrap();
        let valid = &fixture["installs"][0];
        for (data, detail) in [
            (
                json!({"schema_version": 2}),
                "was written by a newer my-skills",
            ),
            (
                json!({"schema_version": "2"}),
                "was written by a newer my-skills",
            ),
            (
                json!({"installs": [valid, 42]}),
                "contains a malformed install record at index 1",
            ),
            (
                json!({"installs": [valid, {"skill": "sample"}]}),
                "install record at index 1 is missing required field(s): destination, host, installed_at, installed_hash, mode, source, source_hash",
            ),
        ] {
            fs::write(&path, data.to_string()).unwrap();
            let error = State::load(Some(&path)).unwrap_err();
            assert!(matches!(error, Error::State(_)));
            assert_eq!(
                error.to_string(),
                format!(
                    "state file {} {detail}; upgrade the CLI, or move {} aside and re-run install",
                    path.display(),
                    path.display()
                )
            );
        }
    }

    #[test]
    fn state_rejects_bad_json_and_wrong_field_types() {
        let root = tempdir().unwrap();
        let path = root.path().join("state.json");
        let mut fixture: Value = serde_json::from_slice(LEGACY_STATE).unwrap();
        fixture["installs"][0]["mode"] = json!("invalid");
        for (data, detail) in [
            ("{".to_owned(), "contains invalid JSON"),
            ("[]".to_owned(), "must contain a JSON object"),
            (
                r#"{"schema_version":"invalid"}"#.to_owned(),
                "has an invalid schema_version",
            ),
            (
                r#"{"installs":null}"#.to_owned(),
                "must contain an installs array",
            ),
            (
                fixture.to_string(),
                "contains a malformed install record at index 0",
            ),
        ] {
            fs::write(&path, data).unwrap();
            let error = State::load(Some(&path)).unwrap_err();
            assert!(matches!(error, Error::State(_)));
            assert!(error.to_string().contains(detail));
        }
    }

    #[test]
    fn state_failed_save_preserves_existing_state_and_cleans_owned_temp() {
        let root = tempdir().unwrap();
        let path = root.path().join("state.json");
        let temp_path = path.with_extension("json.tmp");
        fs::write(&path, LEGACY_STATE).unwrap();
        let mut state = State::load(Some(&path)).unwrap();
        state.remove("alpha", "claude");
        fs::write(&temp_path, "another writer").unwrap();
        assert!(matches!(state.save(), Err(Error::Io { path, .. }) if path == temp_path));
        assert_eq!(fs::read(&path).unwrap(), LEGACY_STATE);
        assert_eq!(fs::read_to_string(&temp_path).unwrap(), "another writer");
        fs::remove_file(&temp_path).unwrap();
        state.save().unwrap();
        assert_eq!(State::load(Some(&path)).unwrap(), state);
        assert!(!temp_path.exists());

        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        assert!(matches!(state.save(), Err(Error::Io { path: actual, .. }) if actual == path));
        assert!(path.is_dir());
        assert!(!temp_path.exists());
    }

    #[test]
    fn data_path_is_shared_and_create_is_opt_in() {
        with_temp_home("data_path_is_shared_and_create_is_opt_in", |root| {
            #[cfg(windows)]
            let expected = root.join("appdata/my-skills/data/sample");
            #[cfg(not(windows))]
            let expected = root.join("data/my-skills/sample");
            assert_eq!(skill_data_path("sample", false).unwrap(), expected);
            assert!(!expected.parent().unwrap().exists());
            assert_eq!(skill_data_path("sample", true).unwrap(), expected);
            assert!(expected.is_dir());
            assert_eq!(skill_data_path("sample", true).unwrap(), expected);
        });
    }

    #[test]
    fn data_path_rejects_invalid_names_before_creating_directories() {
        with_temp_home(
            "data_path_rejects_invalid_names_before_creating_directories",
            |root| {
                for name in ["", "..", "../escaped", "a/b", r"a\b", "UPPER", "-lead"] {
                    let error = skill_data_path(name, true).unwrap_err();
                    assert!(error.to_string().contains("invalid skill name"));
                }
                assert!(!root.join("data").exists());
                assert!(!root.join("appdata").exists());
            },
        );
    }
}

mod hash_golden {
    use std::collections::BTreeMap;
    use std::fs;
    use std::io::ErrorKind;

    use my_skills::Error;
    use my_skills::drive::hash::{hash_directory, hash_file};
    use serde::Deserialize;
    use tempfile::tempdir;

    #[derive(Deserialize)]
    struct Case {
        name: String,
        files: BTreeMap<String, String>,
        dirs: Vec<String>,
    }

    #[test]
    fn directory_hashes_match_python_v0_4_1() {
        let cases: Vec<Case> =
            serde_json::from_str(include_str!("golden/hash/cases.json")).unwrap();
        let mut golden: BTreeMap<String, String> =
            serde_json::from_str(include_str!("golden/hash/golden.json")).unwrap();
        let temp = tempdir().unwrap();

        for case in cases {
            let root = temp.path().join(&case.name);
            fs::create_dir(&root).unwrap();
            for dir in case.dirs {
                fs::create_dir_all(root.join(dir)).unwrap();
            }
            for (relative_path, encoded) in case.files.into_iter().rev() {
                let path = root.join(relative_path);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, decode_base64(&encoded)).unwrap();
            }

            let expected = golden
                .remove(&case.name)
                .unwrap_or_else(|| panic!("missing golden hash for {}", case.name));
            assert_eq!(hash_directory(&root).unwrap(), expected, "{}", case.name);
        }
        assert!(golden.is_empty(), "unused golden hashes: {golden:?}");
    }

    #[test]
    fn file_hash_is_lowercase_hex_without_a_prefix() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("file.txt");
        fs::write(&path, b"abc").unwrap();

        assert_eq!(
            hash_file(&path).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn missing_paths_are_io_errors() {
        let temp = tempdir().unwrap();
        let missing = temp.path().join("missing");
        for result in [hash_file(&missing), hash_directory(&missing)] {
            let Error::Io { path, source } = result.unwrap_err() else {
                panic!("expected an I/O error for a missing path");
            };
            assert_eq!(path, missing);
            assert_eq!(source.kind(), ErrorKind::NotFound);
        }
    }

    #[cfg(unix)]
    #[test]
    fn directory_hash_skips_symlinks() {
        use std::os::unix::fs::symlink;

        let temp = tempdir().unwrap();
        let root = temp.path().join("root");
        let external = temp.path().join("external");
        fs::create_dir(&root).unwrap();
        fs::create_dir(&external).unwrap();
        fs::write(root.join("SKILL.md"), b"# Fixture\n").unwrap();
        fs::write(external.join("file.txt"), b"external").unwrap();
        let expected = hash_directory(&root).unwrap();

        symlink(&external, root.join("linked-dir")).unwrap();
        symlink(external.join("file.txt"), root.join("linked-file")).unwrap();
        assert_eq!(hash_directory(&root).unwrap(), expected);

        let linked_root = temp.path().join("linked-root");
        symlink(&root, &linked_root).unwrap();
        let empty_root = temp.path().join("empty");
        fs::create_dir(&empty_root).unwrap();
        assert_eq!(
            hash_directory(&linked_root).unwrap(),
            hash_directory(&empty_root).unwrap()
        );
    }

    // macOS filesystems reject invalid UTF-8 names before hashing can see them.
    #[cfg(target_os = "linux")]
    #[test]
    fn non_utf8_relative_path_is_an_io_error() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let temp = tempdir().unwrap();
        let path = temp.path().join(OsStr::from_bytes(b"bad-name-\xff"));
        fs::write(&path, b"content").unwrap();

        let err = hash_directory(temp.path()).unwrap_err();
        assert!(err.to_string().contains("bad-name-"));
        assert!(err.to_string().contains("UTF-8"));
        let Error::Io {
            path: error_path,
            source,
        } = err
        else {
            panic!("expected an I/O error for a non-UTF-8 path");
        };
        assert_eq!(error_path, path);
        assert_eq!(source.kind(), ErrorKind::InvalidData);
    }

    fn decode_base64(encoded: &str) -> Vec<u8> {
        const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

        let sextet = |byte| {
            ALPHABET
                .iter()
                .position(|&digit| digit == byte)
                .expect("invalid base64 fixture") as u8
        };
        assert_eq!(encoded.len() % 4, 0, "expected padded base64");
        let mut bytes = Vec::with_capacity(encoded.len() / 4 * 3);
        for quad in encoded.as_bytes().chunks_exact(4) {
            let a = sextet(quad[0]);
            let b = sextet(quad[1]);
            bytes.push((a << 2) | (b >> 4));
            if quad[2] != b'=' {
                let c = sextet(quad[2]);
                bytes.push((b << 4) | (c >> 2));
                if quad[3] != b'=' {
                    bytes.push((c << 6) | sextet(quad[3]));
                }
            }
        }
        bytes
    }
}

mod w15_lock {
    use std::fs::{self, File, OpenOptions};
    use std::process::{Child, Stdio};

    use crate::common::{Fixture, skill};
    use assert_cmd::assert::OutputAssertExt;
    use fs4::FileExt;
    use my_skills::drive::{hash::hash_directory, state::State};
    use predicates::prelude::predicate;

    fn repo() -> Fixture {
        let fixture = Fixture::repo();
        fixture
            .write_manifest("[defaults]\ninstall_mode = 'copy'\n[skills.alpha]\n[skills.beta]\n");
        for name in ["alpha", "beta"] {
            let source = skill(
                &fixture.registry.join("skills"),
                name,
                "A synthetic test skill.",
                "Test body.",
            );
            fs::create_dir(source.join("assets")).unwrap();
            fs::write(source.join("assets/data.txt"), name.repeat(128 * 1024)).unwrap();
        }
        fixture
    }

    fn launch(fixture: &Fixture, name: &str) -> Child {
        fixture
            .process(env!("CARGO_BIN_EXE_my-skills"))
            .args(["sync", name, "--host", "claude", "--yes"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
    }

    fn assert_installed(fixture: &Fixture, names: &[&str]) {
        let state = State::load(Some(&fixture.state())).unwrap();
        assert_eq!(state.records().count(), names.len());
        let mut entries = fs::read_dir(fixture.host("claude"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect::<Vec<_>>();
        entries.sort();
        assert_eq!(entries, names);
        for name in names {
            let record = state.get(name, "claude").unwrap();
            assert_eq!(
                record.mode,
                my_skills::registry::manifest::InstallMode::Copy
            );
            let digest = hash_directory(&fixture.canonical(name)).unwrap();
            assert_eq!(record.source_hash, digest);
            assert_eq!(record.installed_hash, digest);
            assert_eq!(
                hash_directory(&fixture.host("claude").join(name)).unwrap(),
                digest
            );
        }
    }

    #[test]
    fn concurrent_installs_to_the_same_target_both_succeed() {
        for _ in 0..10 {
            let fixture = repo();
            let held = hold_lock(&fixture);
            let mut a = launch(&fixture, "alpha");
            let mut b = launch(&fixture, "alpha");
            let (a_waiting, a_stderr) = waiting_signal(&mut a);
            let (b_waiting, b_stderr) = waiting_signal(&mut b);
            assert_waiting(a_waiting);
            assert_waiting(b_waiting);
            drop(held);
            a.wait_with_output().unwrap().assert().success();
            b.wait_with_output().unwrap().assert().success();
            assert!(a_stderr.join().unwrap().is_empty());
            assert!(b_stderr.join().unwrap().is_empty());
            assert_installed(&fixture, &["alpha"]);
            assert!(fixture.state().with_file_name("state.json.lock").is_file());
        }
    }

    #[test]
    fn concurrent_installs_to_different_targets_keep_both_records() {
        for _ in 0..10 {
            let fixture = repo();
            let a = launch(&fixture, "alpha");
            let b = launch(&fixture, "beta");
            a.wait_with_output().unwrap().assert().success();
            b.wait_with_output().unwrap().assert().success();
            assert_installed(&fixture, &["alpha", "beta"]);
        }
    }

    fn hold_lock(fixture: &Fixture) -> File {
        let lock = fixture.state().with_file_name("state.json.lock");
        fs::create_dir_all(lock.parent().unwrap()).unwrap();
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock)
            .unwrap();
        FileExt::lock(&file).unwrap();
        file
    }

    #[test]
    fn busy_writers_leave_state_and_hosts_unchanged() {
        let fixture = repo();
        fixture
            .command()
            .args(["sync", "alpha", "--host", "claude", "--yes"])
            .assert()
            .success();
        let state = fs::read(fixture.state()).unwrap();
        fixture
            .command()
            .args(["disable", "alpha"])
            .assert()
            .success();
        let lock = hold_lock(&fixture);
        for args in [
            &["sync", "beta", "--host", "claude", "--yes"][..],
            &["sync", "alpha", "--host", "claude", "--yes"][..],
        ] {
            fixture
                .command()
                .env("MY_SKILLS_STATE_LOCK_TIMEOUT_MS", "5")
                .args(args)
                .assert()
                .code(2)
                .stderr(predicate::str::starts_with("error: state is busy: "))
                .stderr(predicate::str::contains("state.json.lock"));
        }
        fixture
            .command()
            .env("MY_SKILLS_STATE_LOCK_TIMEOUT_MS", "5")
            .args(["hooks", "run", "post-merge", "0"])
            .assert()
            .success()
            .stderr(predicate::str::starts_with("error: state is busy: "))
            .stderr(predicate::str::contains(
                "warning: post-merge could not sync the registry",
            ));
        assert_eq!(state, fs::read(fixture.state()).unwrap());
        assert_installed(&fixture, &["alpha"]);
        drop(lock);
    }

    #[test]
    fn readers_do_not_wait_for_the_state_lock() {
        let fixture = repo();
        fixture
            .command()
            .args(["sync", "alpha", "--host", "claude", "--yes"])
            .assert()
            .success();
        let state = fs::read(fixture.state()).unwrap();
        let lock = hold_lock(&fixture);
        let status = crate::common::json(
            fixture
                .command()
                .env("MY_SKILLS_STATE_LOCK_TIMEOUT_MS", "0")
                .args(["skills", "--json"]),
        );
        assert!(
            status["skills"]
                .as_array()
                .unwrap()
                .iter()
                .any(|row| { row["name"] == "alpha" && row["status"]["claude"] == "FRESH" }),
            "{status}"
        );
        let preview = crate::common::json(
            fixture
                .command()
                .env("MY_SKILLS_STATE_LOCK_TIMEOUT_MS", "0")
                .args(["sync", "--host", "claude", "--dry-run", "--json"]),
        );
        assert_eq!(preview["blocked"], false);
        assert!(
            preview["actions"]
                .as_array()
                .unwrap()
                .iter()
                .any(|row| { row["skill"] == "beta" && row["action"] == "CREATE" })
        );
        fixture
            .command()
            .env("MY_SKILLS_STATE_LOCK_TIMEOUT_MS", "0")
            .args(["sync", "alpha", "--host", "claude", "--check"])
            .assert()
            .success()
            .stderr("");
        assert_eq!(state, fs::read(fixture.state()).unwrap());
        assert_installed(&fixture, &["alpha"]);
        drop(lock);
    }

    #[test]
    fn host_import_takes_the_manifest_lock_first_and_reports_busy_as_json() {
        let fixture = repo();
        let copy = skill(
            &fixture.host("claude"),
            "gamma",
            "A local test skill.",
            "Test body.",
        );
        let manifest_before = fs::read(fixture.manifest()).unwrap();
        let state_lock = hold_lock(&fixture);
        let manifest_lock = fixture.manifest().with_extension("toml.edit-lock");
        fs::write(&manifest_lock, "").unwrap();
        let attempt_import = || {
            let mut command = fixture.command();
            command
                .env_remove("MY_SKILLS_ROOT")
                .env("MY_SKILLS_STATE_LOCK_TIMEOUT_MS", "0")
                .args(["import", "gamma", "--from", "claude", "--enable", "--json"]);
            command
        };

        attempt_import()
            .assert()
            .code(2)
            .stderr(predicate::str::contains("my-skills.toml.edit-lock"));

        fixture
            .command()
            .args(["import", "--from", "claude", "--plan", "--json"])
            .assert()
            .success()
            .stderr("");
        fs::remove_file(&manifest_lock).unwrap();

        let busy = attempt_import()
            .assert()
            .code(2)
            .stderr(predicate::str::starts_with("error: state is busy: "));
        let payload: serde_json::Value = serde_json::from_slice(&busy.get_output().stdout).unwrap();
        assert_eq!(payload["error"]["code"], "state_busy");
        assert_eq!(fs::read(fixture.manifest()).unwrap(), manifest_before);
        assert!(copy.is_dir());
        assert!(!fixture.state().exists());
        assert!(!fixture.canonical("gamma").exists());
        assert!(!fixture.root.join("config/my-skills/root").exists());
        assert!(!manifest_lock.exists());
        drop(state_lock);
    }

    #[test]
    fn one_install_copies_to_two_hosts_and_records_matching_hashes() {
        let fixture = repo();
        fixture
            .command()
            .args(["sync", "alpha", "--yes"])
            .assert()
            .success();
        let state = State::load(Some(&fixture.state())).unwrap();
        assert_eq!(state.records().count(), 2);
        let hash = hash_directory(&fixture.canonical("alpha")).unwrap();
        for host in ["claude", "codex"] {
            let record = state.get("alpha", host).unwrap();
            assert_eq!(record.source_hash, hash);
            assert_eq!(record.installed_hash, hash);
            assert_eq!(
                hash_directory(&fixture.host(host).join("alpha")).unwrap(),
                hash
            );
            assert_eq!(fs::read_dir(fixture.host(host)).unwrap().count(), 1);
        }
    }

    #[test]
    fn a_copy_from_a_linked_canonical_directory_tracks_the_target_bytes() {
        let fixture = repo();
        let target = skill(
            &fixture.root,
            "source",
            "A synthetic test skill.",
            "Test body.",
        );
        let canonical = fixture.canonical("beta");
        // Keep the original canonical entry outside the manifest's skills root.
        fs::rename(&canonical, fixture.root.join("original-beta")).unwrap();
        fs::write(
            target.join("SKILL.md"),
            "---\nname: beta\ndescription: A synthetic test skill.\n---\n\nTest body.",
        )
        .unwrap();
        crate::common::link_dir(&target, &canonical);
        fixture
            .command()
            .args(["sync", "beta", "--host", "claude", "--yes"])
            .assert()
            .success();
        let status = || {
            crate::common::json(fixture.command().args(["skills", "--json"]))["skills"]
                .as_array()
                .unwrap()
                .iter()
                .find(|row| row["name"] == "beta")
                .unwrap()["status"]["claude"]
                .clone()
        };
        assert_eq!(status(), "FRESH");
        fs::write(target.join("asset.txt"), "New content.").unwrap();
        assert_eq!(status(), "STALE");
    }

    #[cfg(windows)]
    #[test]
    fn backup_cleanup_failure_keeps_the_update_applied() {
        let fixture = repo();
        fixture
            .command()
            .args(["sync", "alpha", "--host", "claude", "--yes"])
            .assert()
            .success();
        let host = fixture.host("claude");
        let installed = host.join("alpha");
        let relative = "assets/data.txt";
        let original = installed.join(relative);
        let old_bytes = fs::read(&original).unwrap();
        // Deny deletion of a child of the backup without blocking the rename
        // of its top-level directory. Read-only attributes alone allow deletion.
        use crate::common::acl;
        let account = acl::account();
        acl::deny_delete(&original, &account);

        let source = fixture.canonical("alpha");
        fs::write(source.join(relative), "Updated data.").unwrap();
        let result = fixture
            .command()
            .args(["sync", "alpha", "--host", "claude", "--yes"])
            .output()
            .unwrap();
        let backup = fs::read_dir(&host)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(".alpha.backup-")
            });
        let protected = backup.as_ref().map_or(original, |path| path.join(relative));
        acl::allow_delete(&protected, &account);
        let backup = backup.unwrap_or_else(|| panic!("expected leftover backup; got {result:?}"));
        let backed_up_file = backup.join(relative);

        use my_skills::platform::paths::display_path;
        result
            .assert()
            .success()
            .stdout(predicate::str::contains("updated: alpha -> claude"))
            .stderr(predicate::str::contains("warning: cleanup"))
            .stderr(predicate::str::contains(display_path(&backup)));
        let state = State::load(Some(&fixture.state())).unwrap();
        let record = state.get("alpha", "claude").unwrap();
        let digest = hash_directory(&source).unwrap();
        assert_eq!(record.source_hash, digest);
        assert_eq!(record.installed_hash, digest);
        assert_eq!(hash_directory(&installed).unwrap(), digest);
        assert_eq!(fs::read(backed_up_file).unwrap(), old_bytes);
    }

    fn waiting_signal(
        child: &mut Child,
    ) -> (
        std::sync::mpsc::Receiver<String>,
        std::thread::JoinHandle<String>,
    ) {
        let stderr = child.stderr.take().unwrap();
        let (send, receive) = std::sync::mpsc::channel();
        let drain = std::thread::spawn(move || {
            use std::io::{BufRead, Read};
            let mut stderr = std::io::BufReader::new(stderr);
            let mut line = String::new();
            stderr.read_line(&mut line).unwrap();
            let _ = send.send(line);
            // Drain the rest so the child can still report an error and exit.
            let mut rest = String::new();
            stderr.read_to_string(&mut rest).unwrap();
            rest
        });
        (receive, drain)
    }

    fn assert_waiting(signal: std::sync::mpsc::Receiver<String>) {
        let line = signal
            .recv_timeout(std::time::Duration::from_secs(8))
            .unwrap();
        assert!(
            line.starts_with("Waiting for another my-skills command"),
            "{line:?}"
        );
    }

    #[test]
    fn waiting_writers_respect_a_new_active_registry() {
        for args in [
            &["import", "gamma", "--from", "claude", "--enable"][..],
            &["sync", "alpha", "--host", "claude", "--yes"][..],
            &["bootstrap", "--host", "claude"][..],
        ] {
            let fixture = repo();
            let other = repo();
            let local = skill(
                &fixture.host("claude"),
                "gamma",
                "A local fixture.",
                "Local body.",
            );
            let before = fs::read(fixture.manifest()).unwrap();
            let held = hold_lock(&fixture);
            let mut writer = fixture
                .process(env!("CARGO_BIN_EXE_my-skills"))
                .env_remove("MY_SKILLS_ROOT")
                .args(args)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            let (waiting, stderr) = waiting_signal(&mut writer);
            assert_waiting(waiting);
            fixture
                .command()
                .args(["set-root", other.registry.to_str().unwrap()])
                .assert()
                .success();
            let cache = fixture.root.join("config/my-skills/root");
            let selected = fs::read(&cache).unwrap();
            drop(held);
            writer.wait_with_output().unwrap().assert().code(2);
            let stderr = stderr.join().unwrap();
            assert_eq!(
                stderr.matches("active registry cache changed").count(),
                1,
                "{stderr}"
            );
            assert!(stderr.contains("refusing to write"), "{stderr}");
            assert_eq!(fs::read(&cache).unwrap(), selected);
            assert_eq!(fs::read(fixture.manifest()).unwrap(), before);
            assert!(!fixture.state().exists());
            assert!(!fixture.canonical("gamma").exists());
            assert!(!fixture.host("claude").join("alpha").exists());
            assert!(local.is_dir());
        }
    }

    #[test]
    fn waiting_writer_keeps_a_new_selection_of_the_same_registry() {
        let fixture = repo();
        let held = hold_lock(&fixture);
        let mut writer = fixture
            .process(env!("CARGO_BIN_EXE_my-skills"))
            .env_remove("MY_SKILLS_ROOT")
            .args(["sync", "alpha", "--host", "claude", "--yes"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let (waiting, stderr) = waiting_signal(&mut writer);
        assert_waiting(waiting);
        fixture
            .command()
            .args(["set-root", fixture.registry.to_str().unwrap()])
            .assert()
            .success();
        let cache = fixture.root.join("config/my-skills/root");
        let chosen = fs::read(&cache).unwrap();
        drop(held);
        writer.wait_with_output().unwrap().assert().success();
        assert_eq!(
            stderr
                .join()
                .unwrap()
                .matches("active registry cache changed")
                .count(),
            1
        );
        assert_eq!(fs::read(cache).unwrap(), chosen);
        assert_installed(&fixture, &["alpha"]);
    }

    fn shared_host_fixture(codex_mode: &str) -> Fixture {
        let fixture = repo();
        let claude = fixture.host("claude");
        fs::create_dir_all(&claude).unwrap();
        crate::common::link_dir(&claude, &fixture.host("codex"));
        let manifest = fixture.manifest();
        let text = fs::read_to_string(&manifest).unwrap().replace(
            "[targets.codex]\n",
            &format!("[targets.codex]\ninstall_mode = '{codex_mode}'\n"),
        );
        fs::write(manifest, text).unwrap();
        fixture
    }

    #[test]
    fn aliased_copy_targets_install_and_update_one_tree_with_both_records() {
        let fixture = shared_host_fixture("copy");
        fixture
            .command()
            .args(["sync", "alpha", "--yes"])
            .assert()
            .success()
            .stdout(predicate::str::contains("created: alpha -> claude"))
            .stdout(predicate::str::contains("created: alpha -> codex"));
        let source = fixture.canonical("alpha");
        fs::write(source.join("assets/data.txt"), "Updated.").unwrap();
        fixture
            .command()
            .args(["sync", "alpha", "--yes"])
            .assert()
            .success();
        let state = State::load(Some(&fixture.state())).unwrap();
        assert_eq!(state.records().count(), 2);
        for host in ["claude", "codex"] {
            let record = state.get("alpha", host).unwrap();
            assert_eq!(record.source_hash, hash_directory(&source).unwrap());
            assert_eq!(record.source_hash, record.installed_hash);
            assert_eq!(
                record.installed_hash,
                hash_directory(&fixture.host(host).join("alpha")).unwrap()
            );
            assert_eq!(fs::read_dir(fixture.host(host)).unwrap().count(), 1);
        }
    }

    #[test]
    fn shared_copy_rolls_back_the_tree_and_both_records_on_save_failure() {
        let fixture = shared_host_fixture("copy");
        let state = fixture.state();
        fs::create_dir_all(state.parent().unwrap()).unwrap();
        let blocker = state.with_file_name("state.json.tmp");
        fs::write(&blocker, "Reserved staging file.").unwrap();

        fixture
            .command()
            .args(["sync", "alpha", "--yes"])
            .assert()
            .code(1)
            .stdout(predicate::str::contains("state.json.tmp"));
        assert!(!state.exists());
        assert_eq!(fs::read_dir(fixture.host("claude")).unwrap().count(), 0);
        assert_eq!(
            fs::read_to_string(blocker).unwrap(),
            "Reserved staging file."
        );
    }

    #[test]
    fn aliased_targets_with_different_modes_cannot_create_a_shared_entry() {
        let fixture = shared_host_fixture("link");
        let preview =
            crate::common::json(
                fixture
                    .command()
                    .args(["sync", "alpha", "--dry-run", "--json"]),
            );
        assert_eq!(preview["blocked"], true);
        fixture
            .command()
            .args(["sync", "alpha", "beta", "--yes"])
            .assert()
            .code(1)
            .stdout(predicate::str::contains("want different install modes"));
        assert!(!fixture.state().exists());
        assert_eq!(fs::read_dir(fixture.host("claude")).unwrap().count(), 0);
    }

    #[cfg(windows)]
    #[test]
    fn forced_host_import_reports_when_committed_backup_cannot_be_cleaned() {
        let fixture = repo();
        let local = skill(
            &fixture.host("claude"),
            "alpha",
            "A local fixture.",
            "The shared version.",
        );
        let canonical = fixture.canonical("alpha");
        let protected = canonical.join("assets/data.txt");
        let original = fs::read(&protected).unwrap();
        use crate::common::acl;
        let account = acl::account();
        acl::deny_delete(&protected, &account);
        let result = fixture
            .command()
            .args(["import", "alpha", "--from", "claude", "--enable", "--force"])
            .output()
            .unwrap();
        let backup = fs::read_dir(canonical.parent().unwrap())
            .unwrap()
            .map(|entry| entry.unwrap().path().join("previous"))
            .find(|entry| entry.join("assets/data.txt").is_file());
        let protected = backup
            .as_ref()
            .map_or(protected, |backup| backup.join("assets/data.txt"));
        acl::allow_delete(&protected, &account);
        let backup = backup.unwrap_or_else(|| panic!("expected retained backup; got {result:?}"));
        result
            .assert()
            .success()
            .stdout(predicate::str::contains("imported: alpha"))
            .stderr(predicate::str::contains("warning: cleanup"))
            .stderr(predicate::str::contains(
                my_skills::platform::paths::display_path(&backup),
            ));
        assert_eq!(original, fs::read(backup.join("assets/data.txt")).unwrap());
        assert_eq!(
            fs::read(canonical.join("SKILL.md")).unwrap(),
            fs::read(local.join("SKILL.md")).unwrap()
        );
        let state = State::load(Some(&fixture.state())).unwrap();
        assert_eq!(
            state.get("alpha", "claude").unwrap().installed_hash,
            hash_directory(&local).unwrap()
        );
    }
}

mod w15_root_cache {
    //! Wave 15: an explicit set-root reports a root cache it could not save.

    use std::fs;

    use crate::common::Fixture;
    use predicates::prelude::*;
    use predicates::str::contains;

    #[test]
    fn set_root_fails_when_the_root_cache_lock_cannot_be_opened() {
        let fixture = Fixture::repo();
        fixture.write_manifest("");
        let other = fixture.root.join("registry-b");
        fs::create_dir_all(other.join("skills")).unwrap();
        fs::copy(fixture.manifest(), other.join("my-skills.toml")).unwrap();

        fixture
            .command()
            .env_remove("MY_SKILLS_ROOT")
            .args(["set-root"])
            .arg(&fixture.registry)
            .assert()
            .success();
        let cache = fixture.root.join("config").join("my-skills").join("root");
        let before = fs::read(&cache).unwrap();

        // A directory where the lock file belongs makes the lock open fail.
        let lock = cache.with_file_name("root.lock");
        let _ = fs::remove_file(&lock);
        fs::create_dir_all(&lock).unwrap();
        fixture
            .command()
            .env_remove("MY_SKILLS_ROOT")
            .args(["set-root"])
            .arg(&other)
            .assert()
            .code(2)
            .stdout(contains("Active registry root set to").not())
            .stderr(contains("error:"));
        assert_eq!(fs::read(&cache).unwrap(), before);
    }
}
