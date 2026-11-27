//! `doctor` text and JSON reports.

mod common;

mod doctor_report {
    use std::fs;
    use std::path::{Path, PathBuf};

    use crate::common::{Fixture, MANIFEST, skill_md as skill};
    use crate::common::{stdout, stdout_json};
    use assert_cmd::prelude::*;
    use my_skills::commands::doctor::agy_windows_link_warning;
    use my_skills::drive::hash::hash_directory;
    use my_skills::drive::state::{InstallRecord, State};
    use my_skills::platform::paths;
    use my_skills::registry::manifest::{CliOverrides, InstallMode, load_manifest};
    use serde_json::json;

    const REGISTERED: &str = concat!(
        "schema_version = 1\nskills_root = \"skills\"\n",
        "[skills.example]\nenabled = true\nhosts = [\"codex\"]\n",
        "[skills.disabled]\nenabled = false\nhosts = [\"codex\"]\n"
    );

    #[test]
    fn test_doctor_reports_registry_source_version_state_and_data() {
        let fixture = Fixture::isolated();
        fixture.write("my-skills.toml", REGISTERED);
        fixture.cache(&fixture.registry);
        let result = fixture
            .command()
            .env_remove("MY_SKILLS_ROOT")
            .current_dir(fixture.root.join("outside"))
            .args(["doctor", "--no-update-check"])
            .assert()
            .success();
        let text = stdout(&result);

        #[cfg(unix)]
        let data = fixture.root.join("data").join("my-skills");
        #[cfg(windows)]
        let data = fixture.root.join("data").join("my-skills").join("data");
        for line in [
            format!("my-skills {}", env!("CARGO_PKG_VERSION")),
            "Update:  skipped".to_owned(),
            format!(
                "Registry: {} (source: cache)",
                paths::display_path(&fixture.registry.canonicalize().unwrap())
            ),
            "Skills:   2 registered, 1 enabled".to_owned(),
            format!(
                "State:    {}",
                paths::display_path(&fixture.root.join("state/my-skills/state.json"))
            ),
            format!("Data:     {}", paths::display_path(&data)),
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
            .current_dir(fixture.root.join("outside"))
            .args(["doctor", "--no-update-check"])
            .assert()
            .success();
        let text = stdout(&result);
        let expected = format!(
            "Registry: not configured\n{}\nRegistry already on this machine? Run 'my-skills set-root <path>' or set MY_SKILLS_ROOT.",
            my_skills::commands::first_run::NEXT_STEPS
        );
        assert!(text.contains(&expected), "unexpected doctor output: {text}");
        assert!(text.contains("Manifest: not checked"));
        assert!(!fixture.root.join("config").exists());
    }

    #[test]
    fn test_doctor_reports_invalid_env_root_as_manifest_error() {
        let fixture = Fixture::isolated();
        let result = fixture
            .command()
            .env("MY_SKILLS_ROOT", fixture.root.join("missing"))
            .args(["doctor", "--no-update-check"])
            .assert()
            .code(1);
        let text = stdout(&result);
        assert!(text.contains("Registry: INVALID (MY_SKILLS_ROOT="));
        assert!(text.contains("Manifest: INVALID (MY_SKILLS_ROOT="));
        let result = fixture
            .command()
            .env("MY_SKILLS_ROOT", fixture.root.join("missing"))
            .args(["doctor", "--no-update-check", "--json"])
            .assert()
            .code(1);
        let report = stdout_json(&result);
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
            .current_dir(fixture.root.join("outside"))
            .args(["doctor", "--no-update-check", "--json"])
            .assert()
            .success();
        let report = stdout_json(&result);
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
                        &format!(
                            "{MANIFEST}[targets.agy]\nenabled = true\npath = \"~/test-agy\"\n"
                        ),
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
                let line = stdout(&result)
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
                    line.ends_with(&paths::display_path(&fixture.root.join("home").join(path))),
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
        assert!(!fixture.root.join("config").exists());

        let active = fixture.root.join("active");
        fs::create_dir(&active).unwrap();
        fs::write(active.join("my-skills.toml"), MANIFEST).unwrap();
        fixture.cache(&active);
        let result = fixture
            .command()
            .env_remove("MY_SKILLS_ROOT")
            .args(["doctor", "--no-update-check", "--json"])
            .assert()
            .success();
        let report = stdout_json(&result);
        assert_eq!(report["registry"]["source"], "cwd");
        assert!(
            report["registry"]["warning"]
                .as_str()
                .unwrap()
                .contains("this directory is not the active registry")
        );
        assert_eq!(
            fs::read_to_string(fixture.root.join("config/my-skills/root"))
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
        let report = stdout_json(&result);
        assert_eq!(
            report["registry"]["path"],
            paths::display_path(&fixture.registry.canonicalize().unwrap())
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
        let report = stdout_json(&result);

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
                {"name": "post-commit", "status": "installed", "version": "v1"},
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
        let report = stdout_json(&result);
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
        let text = stdout(&result);
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
        "{REGISTERED}[targets.claude]\nenabled = false\n[targets.agy]\nenabled = false\n[targets.codex]\npath = \"~/target\"\n[skills.broken]\nhosts = [\"codex\"]\n"
    ));
        fixture.write("skills/example/SKILL.md", &skill("example"));
        fixture.write(
            "skills/broken/reference.txt",
            "pre-existing skill without SKILL.md\n",
        );
        fixture.write(
            "skills/disabled/reference.txt",
            "disabled skill without SKILL.md\n",
        );
        fixture.commit();

        let intended = fixture.root.join("home/target");
        let source = fixture.registry.join("skills/example");
        let copy = intended.join("example");
        fs::create_dir_all(&copy).unwrap();
        fs::copy(source.join("SKILL.md"), copy.join("SKILL.md")).unwrap();
        let hash = hash_directory(&source).unwrap();
        let state_path = fixture.root.join("state/my-skills/state.json");
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
        let report = stdout_json(&result);
        assert_eq!(
            report["connections"],
            json!({
                "scope": "registry",
                "link": 1, "copy": 1, "fallback": 1,
                "other": {"link": 0, "copy": 0, "fallback": 0},
                // `disabled` is neither wanted nor connected, so it is not MISSING.
                "statuses": {"FRESH": 1, "MISSING": 1},
                "warnings": [],
            })
        );
        assert_eq!(report["full_validation"]["checked"], 2);
        let failures = report["full_validation"]["failures"].as_array().unwrap();
        assert_eq!(failures.len(), 1);
        assert_eq!(
            failures[0],
            json!({"skill": "broken", "errors": ["SKILL.md not found"]})
        );
        let result = fixture
            .command()
            .args(["doctor", "--no-update-check"])
            .assert()
            .success();
        assert!(stdout(&result).contains("Connections: link 1 / copy 1 (fallback 1)"));
        assert!(stdout(&result).contains("Full validation: 2 checked, 1 failed"));
        assert!(stdout(&result).contains("[FAIL] broken: SKILL.md not found"));
    }

    #[test]
    fn doctor_tolerates_invalid_connection_state_and_non_git_registries() {
        let fixture = Fixture::isolated();
        let state = fixture.root.join("state/my-skills/state.json");
        fs::create_dir_all(state.parent().unwrap()).unwrap();
        fs::write(state, "{broken").unwrap();
        let result = fixture
            .command()
            .args(["doctor", "--no-update-check", "--json"])
            .assert()
            .success();
        let report = stdout_json(&result);
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

    #[test]
    fn doctor_displays_registry_path_without_windows_verbatim_prefix() {
        let fixture = Fixture::isolated();
        let result = fixture
            .command()
            .args(["doctor", "--no-update-check"])
            .assert()
            .success();
        let registry_line = stdout(&result)
            .lines()
            .find(|line| line.starts_with("Registry: "))
            .expect("Registry line present");
        assert!(
            !registry_line.contains(r"\\?\"),
            "Registry line should not contain verbatim prefix: {registry_line}"
        );
        assert_eq!(
            registry_line,
            format!(
                "Registry: {} (source: env)",
                paths::display_path(&fixture.registry.canonicalize().unwrap())
            )
        );
    }

    fn sample_record(host: &str, mode: InstallMode) -> InstallRecord {
        InstallRecord {
            skill: "example".into(),
            host: host.into(),
            mode,
            source: "/fixture/src".into(),
            destination: "/fixture/dst".into(),
            source_hash: "sha256:00".into(),
            installed_hash: "sha256:00".into(),
            installed_at: "2026-01-01T00:00:00Z".into(),
            source_type: "canonical".into(),
            source_url: String::new(),
            source_revision: String::new(),
            link_kind: None,
            fallback_reason: None,
        }
    }

    #[test]
    fn agy_windows_link_warning_decision_is_os_and_mode_aware() {
        let fixture = Fixture::isolated();
        let empty_state = State::load(Some(&fixture.root.join("empty-state.json"))).unwrap();

        // No manifest: no warning.
        assert!(agy_windows_link_warning("windows", None, Some(&empty_state)).is_none());

        // agy disabled (default): no warning even on Windows with link default.
        let disabled_manifest = load_manifest(&fixture.registry, &CliOverrides::default()).unwrap();
        assert!(
            agy_windows_link_warning("windows", Some(&disabled_manifest), Some(&empty_state))
                .is_none()
        );

        // agy enabled with effective mode = link and no link connections: warns about setting only.
        fixture.write(
            "my-skills.toml",
            &format!(
                "{MANIFEST}[defaults]\ninstall_mode = \"link\"\n[targets.agy]\nenabled = true\n"
            ),
        );
        let link_manifest = load_manifest(&fixture.registry, &CliOverrides::default()).unwrap();
        let warning =
            agy_windows_link_warning("windows", Some(&link_manifest), Some(&empty_state)).unwrap();
        assert_eq!(
            warning,
            "Antigravity CLI (agy) on Windows may not discover junction-linked skills; \
         set `[targets.agy] install_mode = \"copy\"` in my-skills.local.toml."
        );
        assert!(
            agy_windows_link_warning("macos", Some(&link_manifest), Some(&empty_state)).is_none()
        );
        assert!(
            agy_windows_link_warning("linux", Some(&link_manifest), Some(&empty_state)).is_none()
        );

        // agy enabled with effective mode = link AND existing agy link: suggests both setting and sync.
        let mut agy_link_state = empty_state.clone();
        agy_link_state.put(sample_record("agy", InstallMode::Link));
        let both_warning =
            agy_windows_link_warning("windows", Some(&link_manifest), Some(&agy_link_state))
                .unwrap();
        assert_eq!(
            both_warning,
            "Antigravity CLI (agy) on Windows may not discover junction-linked skills; \
         set `[targets.agy] install_mode = \"copy\"` in my-skills.local.toml \
         and run `my-skills sync --host agy`."
        );

        // agy enabled with [targets.agy] install_mode = "copy" and no link connections: no warning.
        fixture.write(
            "my-skills.local.toml",
            "[targets.agy]\ninstall_mode = \"copy\"\n",
        );
        let copy_manifest = load_manifest(&fixture.registry, &CliOverrides::default()).unwrap();
        assert!(
            agy_windows_link_warning("windows", Some(&copy_manifest), Some(&empty_state)).is_none()
        );

        // Copy connections on agy or link connections on claude do not trigger the warning.
        let mut non_agy_link_state = empty_state.clone();
        non_agy_link_state.put(sample_record("agy", InstallMode::Copy));
        non_agy_link_state.put(sample_record("claude", InstallMode::Link));
        assert!(
            agy_windows_link_warning("windows", Some(&copy_manifest), Some(&non_agy_link_state))
                .is_none()
        );

        // Effective mode already copy + existing agy link: suggests ONLY sync (not local overlay).
        let copy_with_link_warning =
            agy_windows_link_warning("windows", Some(&copy_manifest), Some(&agy_link_state))
                .unwrap();
        assert_eq!(
            copy_with_link_warning,
            "Antigravity CLI (agy) on Windows may not discover junction-linked skills; \
         run `my-skills sync --host agy`."
        );
        assert!(
            agy_windows_link_warning("macos", Some(&copy_manifest), Some(&agy_link_state))
                .is_none()
        );

        // Disabled agy with existing agy link (created via explicit --host agy) still warns on Windows.
        let disabled_with_link_warning =
            agy_windows_link_warning("windows", Some(&disabled_manifest), Some(&agy_link_state))
                .unwrap();
        assert_eq!(
            disabled_with_link_warning,
            "Antigravity CLI (agy) on Windows may not discover junction-linked skills; \
         set `[targets.agy] install_mode = \"copy\"` in my-skills.local.toml \
         and run `my-skills sync --host agy`."
        );
        assert!(
            agy_windows_link_warning("macos", Some(&disabled_manifest), Some(&agy_link_state))
                .is_none()
        );

        // No manifest + existing agy link: fix the registry, set copy mode, then sync.
        let no_manifest_warning =
            agy_windows_link_warning("windows", None, Some(&agy_link_state)).unwrap();
        assert_eq!(
            no_manifest_warning,
            "Antigravity CLI (agy) on Windows may not discover junction-linked skills. \
         First check the Registry line and fix the registry or run `my-skills set-root <path>`, \
         set `[targets.agy] install_mode = \"copy\"` in my-skills.local.toml, \
         and run `my-skills sync --host agy`."
        );
    }

    #[test]
    fn doctor_cli_reports_windows_agy_link_warning_without_failing() {
        let fixture = Fixture::isolated();
        fixture.write(
            "my-skills.toml",
            &format!(
                "{MANIFEST}[defaults]\ninstall_mode = \"link\"\n[targets.agy]\nenabled = true\n"
            ),
        );
        let result = fixture
            .command()
            .args(["doctor", "--no-update-check"])
            .assert()
            .success();
        let text = stdout(&result);
        #[cfg(windows)]
        assert!(
            text.contains("warning: Antigravity CLI (agy) on Windows")
                && text.contains("[targets.agy] install_mode = \"copy\"")
                && text.contains("my-skills.local.toml"),
            "{text}"
        );
        #[cfg(not(windows))]
        assert!(!text.contains("Antigravity CLI (agy) on Windows"), "{text}");

        // Setting [targets.agy] install_mode = "copy" in my-skills.local.toml clears the warning.
        fixture.write(
            "my-skills.local.toml",
            "[targets.agy]\ninstall_mode = \"copy\"\n",
        );
        let result = fixture
            .command()
            .args(["doctor", "--no-update-check"])
            .assert()
            .success();
        let text = stdout(&result);
        assert!(!text.contains("Antigravity CLI (agy) on Windows"), "{text}");
    }

    #[test]
    fn doctor_cli_warns_for_existing_agy_link_when_mode_is_copy_or_disabled() {
        let fixture = Fixture::isolated();
        fixture.write(
            "my-skills.toml",
            &format!(
                "{MANIFEST}[defaults]\ninstall_mode = \"link\"\n[targets.agy]\nenabled = true\n"
            ),
        );
        fixture.write(
            "my-skills.local.toml",
            "[targets.agy]\ninstall_mode = \"copy\"\n",
        );
        let state_path = fixture.root.join("state/my-skills/state.json");
        fs::create_dir_all(state_path.parent().unwrap()).unwrap();
        fs::write(
            &state_path,
            json!({
                "schema_version": 1,
                "installs": [sample_record("agy", InstallMode::Link)]
            })
            .to_string(),
        )
        .unwrap();

        // Effective mode is already copy, so doctor suggests only sync.
        let result = fixture
            .command()
            .args(["doctor", "--no-update-check"])
            .assert()
            .success();
        let text = stdout(&result);
        #[cfg(windows)]
        assert!(
            text.contains("warning: Antigravity CLI (agy) on Windows")
                && text.contains("run `my-skills sync --host agy`.")
                && !text.contains("my-skills.local.toml")
                && !text.contains("set-root")
                && !text.contains("Registry line"),
            "{text}"
        );
        #[cfg(not(windows))]
        assert!(!text.contains("Antigravity CLI (agy) on Windows"), "{text}");

        // Disabled agy in manifest with an existing agy link record still warns on Windows.
        fs::remove_file(fixture.registry.join("my-skills.local.toml")).unwrap();
        fixture.write(
            "my-skills.toml",
            &format!("{MANIFEST}[defaults]\ninstall_mode = \"link\"\n"),
        );
        let result = fixture
            .command()
            .args(["doctor", "--no-update-check"])
            .assert()
            .success();
        let text = stdout(&result);
        #[cfg(windows)]
        assert!(
            text.contains("warning: Antigravity CLI (agy) on Windows")
                && text.contains("and run `my-skills sync --host agy`.")
                && text.contains("[targets.agy] install_mode = \"copy\""),
            "{text}"
        );
        #[cfg(not(windows))]
        assert!(!text.contains("Antigravity CLI (agy) on Windows"), "{text}");
    }

    #[test]
    fn doctor_omits_python_field_in_text_and_json() {
        let fixture = Fixture::isolated();
        let text_res = fixture
            .command()
            .args(["doctor", "--no-update-check"])
            .assert()
            .success();
        assert!(!stdout(&text_res).contains("Python:"));

        let json_res = fixture
            .command()
            .args(["doctor", "--no-update-check", "--json"])
            .assert()
            .success();
        let report = stdout_json(&json_res);
        assert!(report.get("python").is_none(), "{report}");
    }
}

mod v142_doctor {
    use std::fs::{self, OpenOptions};
    use std::path::Path;
    use std::time::{Duration, SystemTime};

    use crate::common::{Fixture, MANIFEST, skill_md as skill};
    use crate::common::{stdout, stdout_json};
    use my_skills::platform::paths;
    use serde_json::{Value, json};

    fn record_json(
        skill_name: &str,
        host: &str,
        mode: &str,
        source: &str,
        destination: &Path,
        fallback_reason: Option<&str>,
    ) -> Value {
        let mut obj = json!({
            "skill": skill_name,
            "host": host,
            "mode": mode,
            "source": source,
            "destination": paths::display_path(destination),
            "source_hash": "sha256:00",
            "installed_hash": "sha256:00",
            "installed_at": "2026-01-01T00:00:00Z",
        });
        if let Some(reason) = fallback_reason {
            obj["fallback_reason"] = json!(reason);
        }
        obj
    }

    fn write_state(fixture: &Fixture, installs: Vec<Value>) -> std::path::PathBuf {
        let state_path = fixture.root.join("state/my-skills/state.json");
        fs::create_dir_all(state_path.parent().unwrap()).unwrap();
        fs::write(
            &state_path,
            json!({"schema_version": 1, "installs": installs}).to_string(),
        )
        .unwrap();
        state_path
    }

    fn set_past_mtime(path: &Path) -> SystemTime {
        let past = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        let file = OpenOptions::new().write(true).open(path).unwrap();
        file.set_modified(past).unwrap();
        fs::metadata(path).unwrap().modified().unwrap()
    }

    fn sum_host_connections(report: &Value) -> (u64, u64, u64) {
        report["hosts"]
            .as_array()
            .unwrap()
            .iter()
            .fold((0, 0, 0), |(l, c, f), host| {
                (
                    l + host["connections"]["link"].as_u64().unwrap(),
                    c + host["connections"]["copy"].as_u64().unwrap(),
                    f + host["connections"]["fallback"].as_u64().unwrap(),
                )
            })
    }

    #[test]
    fn t1_cwd_registry_separates_own_and_other_registry_records() {
        let fixture = Fixture::isolated();
        let reg_a = fixture.registry.clone();
        fixture.cache(&reg_a);

        let reg_b = fixture.root.join("registry-b");
        fs::create_dir_all(reg_b.join("skills/beta-one")).unwrap();
        fs::create_dir_all(reg_b.join("skills/alpha-one")).unwrap();
        fs::write(
            reg_b.join("my-skills.toml"),
            concat!(
                "schema_version = 1\nskills_root = \"skills\"\n",
                "[targets.agy]\nenabled = true\n",
                "[skills.beta-one]\nenabled = true\nhosts = [\"claude\"]\n",
                "[skills.alpha-one]\nenabled = true\nhosts = [\"claude\"]\n",
            ),
        )
        .unwrap();
        fs::write(reg_b.join("skills/beta-one/SKILL.md"), skill("beta-one")).unwrap();
        fs::write(reg_b.join("skills/alpha-one/SKILL.md"), skill("alpha-one")).unwrap();

        let dst = fixture.root.join("home/.claude/skills");
        let state_path = write_state(
            &fixture,
            vec![
                record_json(
                    "alpha-one",
                    "claude",
                    "link",
                    &paths::display_path(&reg_a.join("skills/alpha-one")),
                    &dst.join("alpha-one"),
                    None,
                ),
                record_json(
                    "alpha-two",
                    "codex",
                    "link",
                    &paths::display_path(&reg_a.join("skills/alpha-two")),
                    &fixture.root.join("home/.agents/skills/alpha-two"),
                    None,
                ),
                record_json(
                    "alpha-three",
                    "codex",
                    "copy",
                    &paths::display_path(&reg_a.join("skills/alpha-three")),
                    &fixture.root.join("home/.agents/skills/alpha-three"),
                    Some("link unavailable"),
                ),
                record_json(
                    "beta-one",
                    "claude",
                    "link",
                    &paths::display_path(&reg_b.join("skills/beta-one")),
                    &dst.join("beta-one"),
                    None,
                ),
            ],
        );

        let cache_path = fixture.root.join("config/my-skills/root");
        let state_bytes_before = fs::read(&state_path).unwrap();
        let cache_bytes_before = fs::read(&cache_path).unwrap();
        let state_mtime_before = set_past_mtime(&state_path);
        let cache_mtime_before = set_past_mtime(&cache_path);

        let text_res = fixture
            .command()
            .env_remove("MY_SKILLS_ROOT")
            .current_dir(&reg_b)
            .args(["doctor", "--no-update-check"])
            .assert()
            .success();
        let text = stdout(&text_res);
        let lines: Vec<&str> = text.lines().collect();
        let conn_idx = lines
            .iter()
            .position(|line| *line == "Connections: link 1 / copy 0 (fallback 0)")
            .unwrap_or_else(|| panic!("missing Connections line:\n{text}"));
        let foreign_idx = lines
            .iter()
            .position(|line| *line == "  FOREIGN: 1")
            .unwrap_or_else(|| panic!("missing FOREIGN status line:\n{text}"));
        let missing_idx = lines
            .iter()
            .position(|line| *line == "  MISSING: 1")
            .unwrap_or_else(|| panic!("missing MISSING status line:\n{text}"));
        let other_idx = lines
            .iter()
            .position(|line| {
                *line == "  Other registries or stale records: link 2 / copy 1 (fallback 1)"
            })
            .unwrap_or_else(|| panic!("missing Other registries line:\n{text}"));
        assert!(
            conn_idx < foreign_idx && foreign_idx < missing_idx && missing_idx < other_idx,
            "unexpected line order: conn={conn_idx}, foreign={foreign_idx}, missing={missing_idx}, other={other_idx}\n{text}"
        );
        #[cfg(windows)]
        {
            let warning_idx = lines
                .iter()
                .position(|line| line.starts_with("  warning: Antigravity CLI (agy) on Windows"))
                .unwrap_or_else(|| panic!("missing agy connection warning:\n{text}"));
            assert!(
                other_idx < warning_idx,
                "expected warning after Other line: other={other_idx}, warning={warning_idx}\n{text}"
            );
        }

        let json_res = fixture
            .command()
            .env_remove("MY_SKILLS_ROOT")
            .current_dir(&reg_b)
            .args(["doctor", "--no-update-check", "--json"])
            .assert()
            .success();
        let report = stdout_json(&json_res);
        let conn = &report["connections"];
        assert_eq!(conn["scope"], "registry");
        assert_eq!(conn["link"], 1);
        assert_eq!(conn["copy"], 0);
        assert_eq!(conn["fallback"], 0);
        assert_eq!(conn["other"], json!({"link": 2, "copy": 1, "fallback": 1}));
        assert_eq!(conn["statuses"], json!({"FOREIGN": 1, "MISSING": 1}));
        assert_eq!(sum_host_connections(&report), (1, 0, 0));
        assert_eq!(
            conn["link"].as_u64().unwrap() + conn["other"]["link"].as_u64().unwrap(),
            3
        );
        assert_eq!(
            conn["copy"].as_u64().unwrap() + conn["other"]["copy"].as_u64().unwrap(),
            1
        );
        assert_eq!(
            conn["fallback"].as_u64().unwrap() + conn["other"]["fallback"].as_u64().unwrap(),
            1
        );

        // U6: doctor does not touch state.json or root cache.
        assert_eq!(fs::read(&state_path).unwrap(), state_bytes_before);
        assert_eq!(fs::read(&cache_path).unwrap(), cache_bytes_before);
        assert_eq!(
            fs::metadata(&state_path).unwrap().modified().unwrap(),
            state_mtime_before
        );
        assert_eq!(
            fs::metadata(&cache_path).unwrap().modified().unwrap(),
            cache_mtime_before
        );
    }

    #[test]
    fn t2_active_registry_with_only_own_records_matches_exact_output() {
        let fixture = Fixture::isolated();
        fixture.write(
            "my-skills.toml",
            concat!(
                "schema_version = 1\nskills_root = \"skills\"\n",
                "[skills.alpha-one]\nenabled = true\nhosts = [\"claude\"]\n",
                "[skills.alpha-two]\nenabled = true\nhosts = [\"codex\"]\n",
            ),
        );
        fixture.write("skills/alpha-one/SKILL.md", &skill("alpha-one"));
        fixture.write("skills/alpha-two/SKILL.md", &skill("alpha-two"));
        fixture.cache(&fixture.registry);

        write_state(
            &fixture,
            vec![
                record_json(
                    "alpha-one",
                    "claude",
                    "link",
                    &paths::display_path(&fixture.registry.join("skills/alpha-one")),
                    &fixture.root.join("home/.claude/skills/alpha-one"),
                    None,
                ),
                record_json(
                    "alpha-two",
                    "codex",
                    "copy",
                    &paths::display_path(&fixture.registry.join("skills/alpha-two")),
                    &fixture.root.join("home/.agents/skills/alpha-two"),
                    Some("fallback reason"),
                ),
            ],
        );

        let text_res = fixture
            .command()
            .env_remove("MY_SKILLS_ROOT")
            .env("PATH", &fixture.bin)
            .args(["doctor", "--no-update-check"])
            .assert()
            .success();
        let text = stdout(&text_res);
        assert!(
            !text.contains("Other registries or stale records"),
            "unexpected Other line:\n{text}"
        );

        let hosts_start = text.find("Hosts:\n").expect("Hosts section");
        let manifest_start = text.find("\nManifest:").expect("Manifest section");
        let hosts_block = &text[hosts_start..manifest_start];
        let claude_path = paths::display_path(&fixture.root.join("home/.claude/skills"));
        let codex_path = paths::display_path(&fixture.root.join("home/.agents/skills"));
        let agy_path = paths::display_path(&fixture.root.join("home/.gemini/config/skills"));
        let expected_hosts = format!(
            concat!(
                "Hosts:\n",
                "  {:12} exe={:18} enabled=True  writable=True  path={}\n",
                "    mode=link / connections: link 1 / copy 0 (fallback 0)\n",
                "  {:12} exe={:18} enabled=True  writable=True  path={}\n",
                "    mode=link / connections: link 0 / copy 1 (fallback 1)\n",
                "  {:12} exe={:18} enabled=False writable=True  path={}\n",
                "    mode=link / connections: link 0 / copy 0 (fallback 0)\n",
            ),
            "Claude Code",
            "not found",
            claude_path,
            "Codex",
            "not found",
            codex_path,
            "Antigravity CLI",
            "not found",
            agy_path,
        );
        assert_eq!(hosts_block, expected_hosts);

        let conn_start = text.find("Connections:").expect("Connections section");
        let val_start = text
            .find("\n\nFull validation:")
            .expect("Full validation section");
        let conn_block = &text[conn_start..val_start];
        assert_eq!(
            conn_block,
            "Connections: link 1 / copy 1 (fallback 1)\n  MISSING: 2"
        );

        let json_res = fixture
            .command()
            .env_remove("MY_SKILLS_ROOT")
            .args(["doctor", "--no-update-check", "--json"])
            .assert()
            .success();
        let report = stdout_json(&json_res);
        assert_eq!(
            report["connections"],
            json!({
                "scope": "registry",
                "link": 1,
                "copy": 1,
                "fallback": 1,
                "other": {"link": 0, "copy": 0, "fallback": 0},
                "statuses": {"MISSING": 2},
                "warnings": [],
            })
        );
    }

    #[test]
    fn t3_deleted_old_skills_root_and_removed_host_go_to_other_while_missing_skill_stays() {
        let fixture = Fixture::isolated();
        fixture.write(
            "my-skills.toml",
            concat!(
                "schema_version = 1\nskills_root = \"custom-skills\"\n",
                "[skills.vanished-skill]\nenabled = true\nhosts = [\"codex\"]\n",
            ),
        );
        fs::create_dir_all(fixture.registry.join("custom-skills")).unwrap();

        write_state(
            &fixture,
            vec![
                // 1. Deleted registry path -> other (link on claude)
                record_json(
                    "deleted-one",
                    "claude",
                    "link",
                    &paths::display_path(&fixture.root.join("deleted-registry/skills/deleted-one")),
                    &fixture.root.join("home/.claude/skills/deleted-one"),
                    None,
                ),
                // 2. Old skills folder in same registry -> other (fallback copy on codex)
                record_json(
                    "old-dir-one",
                    "codex",
                    "copy",
                    &paths::display_path(&fixture.registry.join("skills/old-dir-one")),
                    &fixture.root.join("home/.agents/skills/old-dir-one"),
                    Some("old fallback"),
                ),
                // 3. Removed host (hermes) in current custom-skills folder -> other (link)
                record_json(
                    "legacy-host-one",
                    "hermes",
                    "link",
                    &paths::display_path(&fixture.registry.join("custom-skills/legacy-host-one")),
                    &fixture.root.join("home/.hermes/skills/legacy-host-one"),
                    None,
                ),
                // 4. Skill folder disappeared from current custom-skills folder -> stays in this registry (non-fallback copy on codex)
                record_json(
                    "vanished-skill",
                    "codex",
                    "copy",
                    &paths::display_path(&fixture.registry.join("custom-skills/vanished-skill")),
                    &fixture.root.join("home/.agents/skills/vanished-skill"),
                    None,
                ),
            ],
        );

        let json_res = fixture
            .command()
            .args(["doctor", "--no-update-check", "--json"])
            .assert()
            .success();
        let report = stdout_json(&json_res);
        assert_eq!(
            report["connections"],
            json!({
                "scope": "registry",
                "link": 0,
                "copy": 1,
                "fallback": 0,
                "other": {"link": 2, "copy": 1, "fallback": 1},
                "statuses": {"MISSING": 1},
                "warnings": [],
            })
        );
        let host_counts = |name: &str| {
            report["hosts"]
                .as_array()
                .unwrap()
                .iter()
                .find(|h| h["name"] == name)
                .unwrap()["connections"]
                .clone()
        };
        assert_eq!(
            host_counts("claude"),
            json!({"link": 0, "copy": 0, "fallback": 0})
        );
        assert_eq!(
            host_counts("codex"),
            json!({"link": 0, "copy": 1, "fallback": 0})
        );
        assert_eq!(
            host_counts("agy"),
            json!({"link": 0, "copy": 0, "fallback": 0})
        );
        assert_eq!(sum_host_connections(&report), (0, 1, 0));

        let text_res = fixture
            .command()
            .args(["doctor", "--no-update-check"])
            .assert()
            .success();
        let text = stdout(&text_res);
        assert!(text.contains("Connections: link 0 / copy 1 (fallback 0)"));
        assert!(text.contains("  Other registries or stale records: link 2 / copy 1 (fallback 1)"));
    }

    #[test]
    fn t4_machine_scope_when_unconfigured_or_manifest_invalid() {
        let fixture = Fixture::isolated();
        write_state(
            &fixture,
            vec![
                record_json(
                    "alpha-one",
                    "claude",
                    "link",
                    &paths::display_path(&fixture.registry.join("skills/alpha-one")),
                    &fixture.root.join("home/.claude/skills/alpha-one"),
                    None,
                ),
                record_json(
                    "alpha-two",
                    "codex",
                    "link",
                    &paths::display_path(&fixture.root.join("other/skills/alpha-two")),
                    &fixture.root.join("home/.agents/skills/alpha-two"),
                    None,
                ),
                record_json(
                    "alpha-three",
                    "codex",
                    "copy",
                    &paths::display_path(&fixture.root.join("other/skills/alpha-three")),
                    &fixture.root.join("home/.agents/skills/alpha-three"),
                    Some("fallback"),
                ),
            ],
        );

        // 1. Unconfigured environment -> exit 0, machine scope.
        let unconf_text = fixture
            .command()
            .env_remove("MY_SKILLS_ROOT")
            .current_dir(fixture.root.join("outside"))
            .args(["doctor", "--no-update-check"])
            .assert()
            .success();
        let text = stdout(&unconf_text);
        assert!(
        text.contains(
            "Connections: link 2 / copy 1 (fallback 1) across all registries (no registry loaded)"
        ),
        "{text}"
    );
        assert!(
            !text.contains("Other registries or stale records"),
            "{text}"
        );

        let unconf_json = fixture
            .command()
            .env_remove("MY_SKILLS_ROOT")
            .current_dir(fixture.root.join("outside"))
            .args(["doctor", "--no-update-check", "--json"])
            .assert()
            .success();
        let report = stdout_json(&unconf_json);
        assert_eq!(report["connections"]["scope"], "machine");
        assert_eq!(report["connections"]["link"], 2);
        assert_eq!(report["connections"]["copy"], 1);
        assert_eq!(report["connections"]["fallback"], 1);
        assert_eq!(report["connections"].get("other"), Some(&Value::Null));

        // 2. Invalid TOML manifest in cwd -> exit 1, machine scope.
        fixture.write("my-skills.toml", "invalid = [\n");
        let invalid_text = fixture
            .command()
            .env_remove("MY_SKILLS_ROOT")
            .current_dir(&fixture.registry)
            .args(["doctor", "--no-update-check"])
            .assert()
            .code(1);
        let text = stdout(&invalid_text);
        assert!(
        text.contains(
            "Connections: link 2 / copy 1 (fallback 1) across all registries (no registry loaded)"
        ),
        "{text}"
    );
        assert!(
            !text.contains("Other registries or stale records"),
            "{text}"
        );

        let invalid_json = fixture
            .command()
            .env_remove("MY_SKILLS_ROOT")
            .current_dir(&fixture.registry)
            .args(["doctor", "--no-update-check", "--json"])
            .assert()
            .code(1);
        let report = stdout_json(&invalid_json);
        assert_eq!(report["connections"]["scope"], "machine");
        assert_eq!(report["connections"]["link"], 2);
        assert_eq!(report["connections"]["copy"], 1);
        assert_eq!(report["connections"]["fallback"], 1);
        assert_eq!(report["connections"].get("other"), Some(&Value::Null));
    }

    #[test]
    fn t5_nested_registries_and_explicit_root_flag() {
        let fixture = Fixture::isolated();
        let outer = fixture.root.join("outer");
        let inner = outer.join("nested/inner");
        fs::create_dir_all(outer.join("skills")).unwrap();
        fs::create_dir_all(inner.join("skills")).unwrap();
        fs::write(outer.join("my-skills.toml"), MANIFEST).unwrap();
        fs::write(inner.join("my-skills.toml"), MANIFEST).unwrap();

        write_state(
            &fixture,
            vec![
                record_json(
                    "outer-a",
                    "claude",
                    "link",
                    &paths::display_path(&outer.join("skills/outer-a")),
                    &fixture.root.join("home/.claude/skills/outer-a"),
                    None,
                ),
                record_json(
                    "outer-b",
                    "codex",
                    "link",
                    &paths::display_path(&outer.join("skills/outer-b")),
                    &fixture.root.join("home/.agents/skills/outer-b"),
                    None,
                ),
                record_json(
                    "inner-a",
                    "claude",
                    "link",
                    &paths::display_path(&inner.join("skills/inner-a")),
                    &fixture.root.join("home/.claude/skills/inner-a"),
                    None,
                ),
                record_json(
                    "inner-b",
                    "codex",
                    "copy",
                    &paths::display_path(&inner.join("skills/inner-b")),
                    &fixture.root.join("home/.agents/skills/inner-b"),
                    Some("inner fallback"),
                ),
            ],
        );

        let outer_display = paths::display_path(&outer.canonicalize().unwrap());
        let inner_display = paths::display_path(&inner.canonicalize().unwrap());

        // Direction 1: active = outer, cwd = inner.
        fixture.cache(&outer);
        let r1 = stdout_json(
            &fixture
                .command()
                .env_remove("MY_SKILLS_ROOT")
                .current_dir(&inner)
                .args(["doctor", "--no-update-check", "--json"])
                .assert()
                .success(),
        );
        assert_eq!(r1["registry"]["path"], inner_display);
        assert_eq!(r1["connections"]["scope"], "registry");
        assert_eq!(r1["connections"]["link"], 1);
        assert_eq!(r1["connections"]["copy"], 1);
        assert_eq!(r1["connections"]["fallback"], 1);
        assert_eq!(
            r1["connections"]["other"],
            json!({"link": 2, "copy": 0, "fallback": 0})
        );

        // Direction 2: active = inner, cwd = outer.
        fixture.cache(&inner);
        let r2 = stdout_json(
            &fixture
                .command()
                .env_remove("MY_SKILLS_ROOT")
                .current_dir(&outer)
                .args(["doctor", "--no-update-check", "--json"])
                .assert()
                .success(),
        );
        assert_eq!(r2["registry"]["path"], outer_display);
        assert_eq!(r2["connections"]["scope"], "registry");
        assert_eq!(r2["connections"]["link"], 2);
        assert_eq!(r2["connections"]["copy"], 0);
        assert_eq!(r2["connections"]["fallback"], 0);
        assert_eq!(
            r2["connections"]["other"],
            json!({"link": 1, "copy": 1, "fallback": 1})
        );

        // --root selects inner while active is outer and cwd is outside.
        fixture.cache(&outer);
        let r3 = stdout_json(
            &fixture
                .command()
                .env_remove("MY_SKILLS_ROOT")
                .current_dir(fixture.root.join("outside"))
                .args([
                    "--root",
                    inner.to_str().unwrap(),
                    "doctor",
                    "--no-update-check",
                    "--json",
                ])
                .assert()
                .success(),
        );
        assert_eq!(r3["registry"]["path"], inner_display);
        assert_eq!(r3["registry"]["source"], "flag");
        assert_eq!(r3["connections"]["link"], 1);
        assert_eq!(r3["connections"]["copy"], 1);
        assert_eq!(r3["connections"]["fallback"], 1);
        assert_eq!(
            r3["connections"]["other"],
            json!({"link": 2, "copy": 0, "fallback": 0})
        );

        // MY_SKILLS_ROOT selects inner while active is outer and cwd is outside.
        let r4 = stdout_json(
            &fixture
                .command()
                .env("MY_SKILLS_ROOT", &inner)
                .current_dir(fixture.root.join("outside"))
                .args(["doctor", "--no-update-check", "--json"])
                .assert()
                .success(),
        );
        assert_eq!(r4["registry"]["path"], inner_display);
        assert_eq!(r4["registry"]["source"], "env");
        assert_eq!(r4["connections"]["link"], 1);
        assert_eq!(r4["connections"]["copy"], 1);
        assert_eq!(r4["connections"]["fallback"], 1);
        assert_eq!(
            r4["connections"]["other"],
            json!({"link": 2, "copy": 0, "fallback": 0})
        );
    }

    #[test]
    fn t6_corrupt_and_missing_state_and_no_persistent_writes() {
        // Part A: corrupt state.json -> warning, other: null, files untouched.
        let fixture = Fixture::isolated();
        fixture.cache(&fixture.registry);
        let state_path = fixture.root.join("state/my-skills/state.json");
        fs::create_dir_all(state_path.parent().unwrap()).unwrap();
        fs::write(&state_path, "{broken").unwrap();

        let cache_path = fixture.root.join("config/my-skills/root");
        let state_bytes_before = fs::read(&state_path).unwrap();
        let cache_bytes_before = fs::read(&cache_path).unwrap();
        let state_mtime_before = set_past_mtime(&state_path);
        let cache_mtime_before = set_past_mtime(&cache_path);

        let text_res = fixture
            .command()
            .env_remove("MY_SKILLS_ROOT")
            .args(["doctor", "--no-update-check"])
            .assert()
            .success();
        let text = stdout(&text_res);
        assert!(text.contains("Connections: link 0 / copy 0 (fallback 0)"));
        assert!(!text.contains("Other registries or stale records"));
        assert!(text.contains("invalid JSON"));

        let json_res = fixture
            .command()
            .env_remove("MY_SKILLS_ROOT")
            .args(["doctor", "--no-update-check", "--json"])
            .assert()
            .success();
        let report = stdout_json(&json_res);
        assert_eq!(report["connections"]["scope"], "registry");
        assert_eq!(report["connections"]["link"], 0);
        assert_eq!(report["connections"]["copy"], 0);
        assert_eq!(report["connections"]["fallback"], 0);
        assert_eq!(report["connections"].get("other"), Some(&Value::Null));
        assert!(
            report["connections"]["warnings"][0]
                .as_str()
                .unwrap()
                .contains("invalid JSON")
        );

        assert_eq!(fs::read(&state_path).unwrap(), state_bytes_before);
        assert_eq!(fs::read(&cache_path).unwrap(), cache_bytes_before);
        assert_eq!(
            fs::metadata(&state_path).unwrap().modified().unwrap(),
            state_mtime_before
        );
        assert_eq!(
            fs::metadata(&cache_path).unwrap().modified().unwrap(),
            cache_mtime_before
        );

        // Part B: missing state.json and no root cache (cwd discovery) -> empty state, no files created.
        let clean = Fixture::isolated();
        let clean_state = clean.root.join("state/my-skills/state.json");
        let clean_cache_dir = clean.root.join("config");
        assert!(!clean_state.exists());
        assert!(!clean_cache_dir.exists());

        let res = clean
            .command()
            .env_remove("MY_SKILLS_ROOT")
            .current_dir(&clean.registry)
            .args(["doctor", "--no-update-check", "--json"])
            .assert()
            .success();
        let report = stdout_json(&res);
        assert_eq!(report["registry"]["source"], "cwd");
        assert_eq!(
            report["connections"],
            json!({
                "scope": "registry",
                "link": 0,
                "copy": 0,
                "fallback": 0,
                "other": {"link": 0, "copy": 0, "fallback": 0},
                "statuses": {},
                "warnings": [],
            })
        );
        assert!(!clean_state.exists());
        assert!(!clean_cache_dir.exists());
    }

    #[cfg(windows)]
    #[test]
    fn t8_windows_verbatim_prefix_and_case_difference_count_as_owned() {
        let fixture = Fixture::isolated();
        fs::create_dir_all(fixture.registry.join("skills/win-verbatim")).unwrap();
        fs::create_dir_all(fixture.registry.join("skills/win-case")).unwrap();

        let verbatim = format!(
            r"\\?\{}",
            paths::display_path(&fixture.registry.join("skills/win-verbatim"))
        );
        let upper = paths::display_path(&fixture.registry.join("skills/win-case")).to_uppercase();

        write_state(
            &fixture,
            vec![
                record_json(
                    "win-verbatim",
                    "claude",
                    "link",
                    &verbatim,
                    &fixture.root.join("home/.claude/skills/win-verbatim"),
                    None,
                ),
                record_json(
                    "win-case",
                    "codex",
                    "copy",
                    &upper,
                    &fixture.root.join("home/.agents/skills/win-case"),
                    None,
                ),
            ],
        );

        let report = stdout_json(
            &fixture
                .command()
                .args(["doctor", "--no-update-check", "--json"])
                .assert()
                .success(),
        );
        assert_eq!(report["connections"]["scope"], "registry");
        assert_eq!(report["connections"]["link"], 1);
        assert_eq!(report["connections"]["copy"], 1);
        assert_eq!(report["connections"]["fallback"], 0);
        assert_eq!(
            report["connections"]["other"],
            json!({"link": 0, "copy": 0, "fallback": 0})
        );
        assert_eq!(sum_host_connections(&report), (1, 1, 0));
    }
}
