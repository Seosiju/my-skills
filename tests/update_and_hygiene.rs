//! Self-update, repository seed smoke tests, and public hygiene.

mod common;

mod update_cli {
    use std::fs;

    use crate::common::update::{CURRENT, NEXT, UpdateFixture as Fixture, archive_name};
    use predicates::prelude::predicate;

    #[test]
    fn update_check_exits_one_when_stable_update_is_available() {
        let fixture = Fixture::new();
        let latest = format!("v{NEXT}");
        fixture.release_list(&[
            ("v2.0.0", false, false),
            ("v1.9999.0", false, true),
            ("v1.9998.0", true, false),
            ("v1.9997.0-rc.1", false, false),
            (&latest, false, false),
            ("v1.2.0", false, false),
            ("v0.4.2", false, false),
            ("v1.invalid", false, false),
        ]);
        fixture
        .run(&["update", "--check"])
        .code(1)
        .stdout(format!(
            "Current: my-skills {CURRENT}\nChannel: stable\nLatest:  {latest}\nUpdate available\n"
        ))
        .stderr("");
    }

    #[test]
    fn update_check_reports_current_main_release_and_ahead_version() {
        let fixture = Fixture::new();
        fixture.release_list(&[(&format!("v{CURRENT}"), true, false)]);
        fixture
            .run(&["update", "--check", "--channel", "main"])
            .success()
            .stdout(predicate::str::contains("Already up to date"));

        fixture.release_list(&[("v0.4.2", false, false)]);
        fixture
            .run(&["update", "--check", "--channel", "main"])
            .success()
            .stdout(predicate::str::contains(
                "Installed version is ahead of the latest main release",
            ));
    }

    #[test]
    fn update_returns_two_when_stable_check_cannot_resolve_releases() {
        let fixture = Fixture::new();
        fixture
            .run(&["update", "--check"])
            .code(2)
            .stderr(predicate::str::contains(
                "Update status unknown: could not fetch GitHub releases",
            ));

        fixture.release_list(&[("v0.4.2", false, false), ("v1.0.0-rc.1", true, false)]);
        fixture
            .run(&["update", "--check"])
            .code(2)
            .stderr("Update status unknown: no stable v1 releases found\n");
    }

    #[test]
    fn update_dry_run_prints_main_channel_downloads_and_writes_nothing() {
        let fixture = Fixture::new();
        // Prereleases of NEXT stay ahead of the package version at every release.
        let (rc2, rc10) = (format!("v{NEXT}-rc.2"), format!("v{NEXT}-rc.10"));
        fixture.release_list(&[
            ("v1.1.0", false, false),
            (&rc2, true, false),
            (&rc10, true, false),
        ]);
        let before = fs::read(&fixture.exe).unwrap();
        let asset_url = fixture.asset_url();
        let archive = asset_url.join(archive_name()).unwrap();
        let checksums = asset_url.join("SHA256SUMS").unwrap();
        fixture
            .run(&["update", "--channel", "main", "--dry-run"])
            .success()
            .stdout(format!(
                "Current: my-skills {CURRENT}\nChannel: main\nLatest:  {rc10}\n\
             Would download: {checksums}\nWould download: {archive}\nWould replace:  {}\n",
                my_skills::platform::paths::display_path(&fixture.exe.canonicalize().unwrap())
            ))
            .stderr("");
        assert_eq!(fs::read(&fixture.exe).unwrap(), before);
        assert_eq!(
            fs::read_dir(fixture.exe.parent().unwrap()).unwrap().count(),
            1
        );
    }

    #[test]
    fn update_installs_archive_and_verifies_stable_version() {
        let fixture = Fixture::new();
        fixture.next_release();
        let candidate = fixture.compile_next_binary();
        fixture.package(&candidate);

        fixture
            .run(&["update"])
            .success()
            .stdout(predicate::str::contains(format!(
                "Updated: my-skills {NEXT}"
            )))
            .stderr("");
        fixture
            .run(&["--version"])
            .success()
            .stdout(format!("my-skills {NEXT}\n"));
        assert_eq!(
            fs::read(&fixture.exe).unwrap(),
            fs::read(candidate).unwrap()
        );
    }

    fn assert_original_executable(fixture: &Fixture, before: &[u8]) {
        assert_eq!(fs::read(&fixture.exe).unwrap(), before);
        fixture
            .run(&["--version"])
            .success()
            .stdout(format!("my-skills {CURRENT}\n"));
    }

    #[test]
    fn update_checksum_mismatch_preserves_original_executable() {
        let fixture = Fixture::new();
        fixture.next_release();
        let before = fs::read(&fixture.exe).unwrap();
        fs::write(fixture.assets.join(archive_name()), b"tampered archive").unwrap();
        fixture.checksum(&"0".repeat(64));
        fixture
            .run(&["update"])
            .code(1)
            .stderr(predicate::str::contains("SHA-256 mismatch"));
        assert_original_executable(&fixture, &before);
    }

    #[test]
    fn update_failed_download_preserves_original_executable() {
        let fixture = Fixture::new();
        fixture.next_release();
        let before = fs::read(&fixture.exe).unwrap();
        fixture.checksum(&"0".repeat(64));
        fixture
            .run(&["update"])
            .code(1)
            .stderr(predicate::str::contains(format!(
                "could not download {}",
                archive_name()
            )));
        assert_original_executable(&fixture, &before);
    }

    #[test]
    fn post_install_version_mismatch_restores_original_executable() {
        let fixture = Fixture::new();
        fixture.next_release();
        let before = fs::read(&fixture.exe).unwrap();
        fixture.package(&fixture.exe);
        fixture
            .run(&["update"])
            .code(1)
            .stderr(predicate::str::contains(format!(
                "updated command did not report expected version {NEXT}: my-skills {CURRENT}"
            )));
        assert_original_executable(&fixture, &before);
    }

    #[test]
    fn doctor_skips_lookup_when_disabled() {
        assert_eq!(
            my_skills::update::update_status_line(true),
            "not checked (--no-update-check)"
        );
    }
}

mod repo_skills_smoke {
    use assert_cmd::Command;
    use std::fs;
    use std::path::{Path, PathBuf};

    use crate::common::Fixture;
    use my_skills::check::validate::validate_skill;
    use my_skills::platform::paths;
    use serde_json::Value;

    fn repo_fixture() -> Fixture {
        let fixture = Fixture::isolated();
        let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
            .canonicalize()
            .unwrap();
        // Copy the working contents of tracked files so local edits still get tested.
        // The allowlist excludes checkout-local manifests and untracked seed files.
        let output = fixture
            .git_command(&["ls-files", "-z", "--", "my-skills.toml", "skills/"])
            .current_dir(&repo)
            .output()
            .expect("list the tool repo's tracked manifest and seed files");
        assert!(
            output.status.success(),
            "git ls-files failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let paths =
            std::str::from_utf8(&output.stdout).expect("tracked seed paths should be UTF-8");
        let mut copied_manifest = false;
        for relative in paths.split_terminator('\0') {
            let source = repo.join(relative);
            assert!(
                source.symlink_metadata().unwrap().is_file()
                    && source.canonicalize().unwrap().starts_with(&repo),
                "expected a regular tracked file inside the tool repo: {}",
                source.display()
            );
            let destination = fixture.registry.join(relative);
            fs::create_dir_all(destination.parent().unwrap()).unwrap();
            fs::copy(&source, &destination)
                .unwrap_or_else(|error| panic!("could not copy {}: {error}", source.display()));
            copied_manifest |= relative == "my-skills.toml";
        }
        assert!(copied_manifest, "the tool repo's manifest is not tracked");
        fixture
    }

    fn seed_skills(registry: &Path) -> Vec<PathBuf> {
        let mut skills: Vec<_> = fs::read_dir(registry.join("skills"))
            .expect("read the copied seed skills directory")
            .map(|entry| entry.expect("read a skills directory entry").path())
            .filter(|path| path.join("SKILL.md").is_file())
            .collect();
        skills.sort();
        assert!(!skills.is_empty(), "the tool repo has no seed skills");
        skills
    }

    #[test]
    fn repo_seed_skills_pass_validation() {
        let fixture = repo_fixture();
        for path in seed_skills(&fixture.registry) {
            let validation = validate_skill(&path);
            assert!(
                validation.ok(),
                "{}: validation errors: {:?}",
                path.display(),
                validation.errors
            );
        }
    }

    fn cli_command(fixture: &Fixture) -> Command {
        let temp = fixture.root.join("outside");
        let mut command = fixture.command();
        command
            .env("APPDATA", fixture.root.join("appdata"))
            .env("TEMP", &temp)
            .env("TMP", &temp)
            .env("TMPDIR", &temp);
        command
    }

    fn cli_stdout(command: &mut Command, args: &[&str]) -> String {
        let output = command
            .args(args)
            .output()
            .unwrap_or_else(|error| panic!("could not run my-skills {args:?}: {error}"));
        assert!(
            output.status.success(),
            "my-skills {args:?} exited with {}; stdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).expect("CLI stdout should be UTF-8")
    }

    fn assert_dir_entries(dir: &Path, expected: &[&str]) {
        let mut entries: Vec<_> = fs::read_dir(dir)
            .unwrap_or_else(|error| panic!("could not read {}: {error}", dir.display()))
            .map(|entry| entry.unwrap().file_name())
            .collect();
        entries.sort();
        assert_eq!(entries, expected, "unexpected entries in {}", dir.display());
    }

    #[test]
    fn repo_manifest_supports_read_only_cli_commands() {
        let fixture = repo_fixture();
        let seeds = seed_skills(&fixture.registry);
        let isolated_dirs = ["config", "state", "data", "appdata"];
        for dir in isolated_dirs {
            fs::create_dir(fixture.root.join(dir)).unwrap();
        }
        let by_env = || {
            let mut command = cli_command(&fixture);
            command.current_dir(fixture.root.join("outside"));
            command
        };

        // The unfiltered catalog contains an entry for each manifest skill.
        let catalog = cli_stdout(&mut by_env(), &["skills", "--json"]);
        let json: Value = serde_json::from_str(&catalog)
            .unwrap_or_else(|error| panic!("invalid skills JSON: {error}\n{catalog}"));
        let entries = json["skills"].as_array().expect("expected a skills array");
        assert_eq!(
            entries.len(),
            seeds.len(),
            "seed count differs from catalog:\n{catalog}"
        );
        for entry in entries {
            assert!(
                entry.get("error").is_none(),
                "invalid manifest skill:\n{entry}"
            );
        }
        for path in &seeds {
            let name = path.file_name().unwrap().to_str().unwrap();
            assert!(
                entries.iter().any(|entry| entry["name"] == name
                    && entry["path"] == Path::new("skills").join(name).to_str().unwrap()),
                "seed skill {name} missing from the CLI catalog:\n{catalog}"
            );
        }

        let enabled = entries
            .iter()
            .filter(|entry| entry["enabled"] == true)
            .count();
        assert!(enabled > 0, "the tool repo has no enabled seed skills");
        let doctor = cli_stdout(&mut by_env(), &["doctor", "--no-update-check"]);
        let registry = fixture.registry.canonicalize().unwrap();
        let displayed_registry = paths::display_path(&registry);
        let env_origin = format!("Registry: {displayed_registry} (source: env)");
        let registrations = format!("Skills:   {} registered, {enabled} enabled", seeds.len());
        let validation = format!("Full validation: {enabled} checked, 0 failed");
        for expected in [
            &env_origin,
            "Manifest: valid",
            "Update:  skipped",
            &registrations,
            &validation,
        ] {
            assert!(
                doctor.lines().any(|line| line == expected),
                "doctor output missing {expected:?}:\n{doctor}"
            );
        }

        let bootstrap = cli_stdout(&mut by_env(), &["bootstrap", "--dry-run"]);
        assert!(
            bootstrap.contains("would run: my-skills sync --yes --dry-run")
                && bootstrap.contains("  git hooks: skipped (not a Git repository)"),
            "{bootstrap}"
        );

        for host in ["claude", "codex"] {
            let plan = cli_stdout(
                &mut by_env(),
                &["sync", "my-skills", "--host", host, "--dry-run"],
            );
            assert!(
                plan.lines().any(|line| line.split_whitespace().take(4).eq([
                    "CREATE",
                    "my-skills",
                    "->",
                    host
                ])),
                "expected a sync action for {host}:\n{plan}"
            );
        }
        let agy_plan = cli_stdout(
            &mut by_env(),
            &["sync", "my-skills", "--host", "agy", "--dry-run"],
        );
        assert!(
            !agy_plan.contains("CREATE"),
            "disabled agy target should not plan a connection:\n{agy_plan}"
        );
        let config = fixture.root.join("config");
        assert_dir_entries(&config, &[]);
        let by_cwd = || {
            let mut command = cli_command(&fixture);
            command.env_remove("MY_SKILLS_ROOT");
            command
        };
        let doctor = cli_stdout(&mut by_cwd(), &["doctor", "--no-update-check"]);
        let cwd_origin = format!("Registry: {displayed_registry} (source: cwd)");
        for expected in [&cwd_origin, &validation] {
            assert!(
                doctor.lines().any(|line| line == expected),
                "doctor output missing {expected:?}:\n{doctor}"
            );
        }
        assert_dir_entries(&config, &[]);

        // First-run CWD discovery during catalog loading leaves the config empty.
        let cwd_catalog = cli_stdout(&mut by_cwd(), &["skills", "--json"]);
        let cwd_json: Value = serde_json::from_str(&cwd_catalog).unwrap();
        assert_eq!(cwd_json["skills"], json["skills"]);
        assert_eq!(cwd_json["registry"]["path"], displayed_registry);
        assert_eq!(cwd_json["registry"]["source"], "cwd");
        assert_dir_entries(&config, &[]);
        for dir in ["home", "state", "data", "appdata", "outside"] {
            assert_dir_entries(&fixture.root.join(dir), &[]);
        }
    }
}

mod public_hygiene {
    //! Public-repository hygiene. Port of `tests/test_public_hygiene.py`, kept in
    //! Rust so it survives removing the Python package (handoff §7 D), and widened
    //! to Rust, shell, and PowerShell sources.
    //!
    //! Private values are assembled from pieces so this file never contains them.

    use std::collections::BTreeSet;
    use std::fs;
    use std::path::Path;
    use std::process::Command;

    const TEXT_SUFFIXES: &[&str] = &[
        "md", "json", "toml", "py", "yaml", "yml", "txt", "rs", "sh", "ps1", "lock", "",
    ];

    fn repo_root() -> &'static Path {
        Path::new(env!("CARGO_MANIFEST_DIR"))
    }

    fn tracked_files() -> Vec<String> {
        let output = Command::new("git")
            .arg("ls-files")
            .current_dir(repo_root())
            .output()
            .expect("git ls-files runs");
        assert!(output.status.success(), "git ls-files failed");
        String::from_utf8(output.stdout)
            .expect("tracked paths are UTF-8")
            .lines()
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect()
    }

    fn tracked_text() -> Vec<(String, String)> {
        tracked_files()
            .into_iter()
            .filter_map(|rel| {
                let path = repo_root().join(&rel);
                let suffix = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                // git ls-files lists unstaged deletions too.
                if !TEXT_SUFFIXES.contains(&suffix) || !path.is_file() {
                    return None;
                }
                fs::read_to_string(&path).ok().map(|text| (rel, text))
            })
            .collect()
    }

    fn private_values() -> Vec<String> {
        vec![
            ["hermes", "forjs"].concat(),
            ["e0ffe", "317", "-5557-43b4-a551-466ff24275a2"].concat(),
            ["ideal", "innov39", "@", "gmail", ".com"].concat(),
            ["712", "020:", "076771c9-2a42-4120-877f-2b10b68c08bd"].concat(),
            ["jun", "seon"].concat(),
        ]
    }

    #[test]
    fn tracked_files_do_not_contain_actual_private_jira_values() {
        let values = private_values();
        let leaks: Vec<String> = tracked_text()
            .iter()
            .flat_map(|(rel, text)| {
                values
                    .iter()
                    .filter(|value| text.contains(value.as_str()))
                    .map(move |value| format!("{rel}: {value}"))
            })
            .collect();
        assert!(leaks.is_empty(), "{leaks:?}");
    }

    #[test]
    fn tracked_files_do_not_contain_actual_machine_home_path() {
        let private_home = ["/Users/", "snu", ".sim"].concat();
        let leaks: Vec<String> = tracked_text()
            .into_iter()
            .filter(|(_, text)| text.contains(&private_home))
            .map(|(rel, _)| rel)
            .collect();
        assert!(leaks.is_empty(), "{leaks:?}");
    }

    #[test]
    fn my_jira_uses_example_config_not_tracked_private_config() {
        let tracked: BTreeSet<String> = tracked_files().into_iter().collect();
        assert!(!tracked.contains("skills/my-jira/config.json"));
        assert!(tracked.contains("skills/my-jira/config.example.json"));
    }

    #[test]
    fn cli_inventory_scratch_files_are_not_tracked() {
        let tracked: BTreeSet<String> = tracked_files().into_iter().collect();
        for scratch in [
            "skills/cli-inventory/cvelist",
            "skills/cli-inventory/cvelist-our",
            "skills/cli-inventory/cvelist.diff",
            "skills/cli-inventory/n",
            "skills/cli-inventory/patcheck.tmp",
        ] {
            assert!(!tracked.contains(scratch), "{scratch} is tracked");
        }
    }

    #[test]
    fn roadmap_links_seed_design() {
        let roadmap = fs::read_to_string(
            repo_root().join("docs/releases/v0.x/2026-06-30-my-skills-open-source-roadmap.md"),
        )
        .unwrap();
        assert!(roadmap.contains("docs/2026-07-01-default-skills-seed-design.md"));
        assert!(roadmap.contains("docs/2026-07-01-seed-implementation-spec.md"));
    }
}
