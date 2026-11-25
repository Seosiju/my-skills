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

    let temp = tempdir().unwrap();
    let root = temp.path();
    let output = Command::new(env::current_exe().unwrap())
        .args(["--exact", test, "--include-ignored", "--nocapture"])
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
