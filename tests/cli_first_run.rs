//! First run, `--root`, and the shared JSON error and output contract.

mod common;

mod cli_smoke {
    use predicates::str::contains;

    use crate::common::Fixture;

    #[test]
    fn version_prints_crate_version() {
        Fixture::new()
            .command()
            .env_remove("MY_SKILLS_ROOT")
            .arg("--version")
            .assert()
            .success()
            .stdout(contains(env!("CARGO_PKG_VERSION")));
    }
}

mod v141_firstrun {
    use std::collections::BTreeSet;
    use std::fs;
    use std::path::Path;

    use crate::common::Fixture;
    use my_skills::commands::first_run::NEXT_STEPS;
    use serde_json::Value;

    fn stdout_text(assertion: &assert_cmd::assert::Assert) -> String {
        String::from_utf8(assertion.get_output().stdout.clone())
            .unwrap()
            .replace("\r\n", "\n")
    }

    fn stderr_text(assertion: &assert_cmd::assert::Assert) -> String {
        String::from_utf8(assertion.get_output().stderr.clone())
            .unwrap()
            .replace("\r\n", "\n")
    }

    fn object_keys(value: &Value) -> BTreeSet<String> {
        value
            .as_object()
            .expect("expected JSON object")
            .keys()
            .cloned()
            .collect()
    }

    fn normalized_lines(text: &str) -> Vec<String> {
        text.replace("\r\n", "\n")
            .lines()
            .map(|line| line.trim_end().to_owned())
            .collect()
    }

    /// T-B1: Unconfigured environment `skills` prints B2 error with exit code 2,
    /// and `skills --json` emits a JSON error with the exact same key set and
    /// `code` as another config error from `skills --json`.
    #[test]
    fn t_b1_unconfigured_read_command_shows_b2_and_json_error() {
        let fixture = Fixture::isolated();
        let outside = fixture.root.join("outside");
        let missing = fixture.root.join("does-not-exist");

        let result = fixture
            .command()
            .env_remove("MY_SKILLS_ROOT")
            .current_dir(&outside)
            .arg("skills")
            .assert()
            .code(2);
        let stderr = stderr_text(&result);
        let expected = format!(
            "error: my-skills.toml not found. No registry is set up on this machine.\n{NEXT_STEPS}\nRegistry already on this machine? Run 'my-skills set-root <path>' or set MY_SKILLS_ROOT.\n"
        );
        assert_eq!(stderr, expected);

        let unconfigured_json = fixture
            .command()
            .env_remove("MY_SKILLS_ROOT")
            .current_dir(&outside)
            .args(["skills", "--json"])
            .assert()
            .code(2);
        let payload: Value =
            serde_json::from_slice(&unconfigured_json.get_output().stdout).unwrap();

        let other_config_json = fixture
            .command()
            .env("MY_SKILLS_ROOT", &missing)
            .current_dir(&outside)
            .args(["skills", "--json"])
            .assert()
            .code(2);
        let reference: Value =
            serde_json::from_slice(&other_config_json.get_output().stdout).unwrap();

        assert_eq!(object_keys(&payload), object_keys(&reference));
        assert_eq!(
            object_keys(&payload["error"]),
            object_keys(&reference["error"])
        );
        assert_eq!(payload["error"]["code"], reference["error"]["code"]);
        assert_eq!(payload["error"]["code"], "config");
        let message = payload["error"]["message"].as_str().unwrap();
        assert!(
            message.contains("New here?"),
            "expected 'New here?' in JSON error message: {payload}"
        );
    }

    /// T-B2: Unconfigured environment `doctor` shows B3 block and `doctor --json`
    /// keeps its existing `registry` object key set (including `warning`).
    #[test]
    fn t_b2_unconfigured_doctor_shows_b3_and_keeps_json_shape() {
        let fixture = Fixture::isolated();
        let outside = fixture.root.join("outside");

        let result = fixture
            .command()
            .env_remove("MY_SKILLS_ROOT")
            .current_dir(&outside)
            .args(["doctor", "--no-update-check"])
            .assert()
            .success();
        let stdout = stdout_text(&result);
        let expected = format!(
            "Registry: not configured\n{NEXT_STEPS}\nRegistry already on this machine? Run 'my-skills set-root <path>' or set MY_SKILLS_ROOT."
        );
        assert!(
            stdout.contains(&expected),
            "unexpected doctor output:\n{stdout}"
        );

        let json_result = fixture
            .command()
            .env_remove("MY_SKILLS_ROOT")
            .current_dir(&outside)
            .args(["doctor", "--no-update-check", "--json"])
            .assert()
            .success();
        let report: Value = serde_json::from_slice(&json_result.get_output().stdout).unwrap();
        let expected_registry_keys: BTreeSet<String> = ["path", "source", "error", "warning"]
            .into_iter()
            .map(str::to_owned)
            .collect();
        assert_eq!(object_keys(&report["registry"]), expected_registry_keys);
        assert!(report["registry"]["path"].is_null());
        assert!(report["registry"]["source"].is_null());
        assert!(report["registry"]["error"].is_null());
        assert!(report["registry"]["warning"].is_null());
        assert_eq!(report["manifest"]["status"], "not_checked");
    }

    /// T-B3: Bare `my-skills` prints help + NEXT_STEPS when unconfigured, help only
    /// when a registry is configured via `MY_SKILLS_ROOT`, `--root`, or cwd
    /// discovery, and never writes the root cache.
    #[test]
    fn t_b3_bare_invocation_guidance_and_no_cache() {
        let fixture = Fixture::isolated();
        let outside = fixture.root.join("outside");

        let unconfigured = fixture
            .command()
            .env_remove("MY_SKILLS_ROOT")
            .current_dir(&outside)
            .assert()
            .success();
        let unconfigured_out = stdout_text(&unconfigured);
        assert!(unconfigured_out.contains("Usage:"));
        assert!(unconfigured_out.ends_with(&format!("\n\n{NEXT_STEPS}\n")));
        assert!(!fixture.root.join("config").exists());

        let with_env = fixture
            .command()
            .env("MY_SKILLS_ROOT", &fixture.registry)
            .current_dir(&outside)
            .assert()
            .success();
        let with_env_out = stdout_text(&with_env);
        assert_eq!(unconfigured_out, format!("{with_env_out}\n{NEXT_STEPS}\n"));
        assert!(!with_env_out.contains("Next:"));
        assert!(!fixture.root.join("config").exists());

        let with_flag = fixture
            .command()
            .env_remove("MY_SKILLS_ROOT")
            .current_dir(&outside)
            .args(["--root", fixture.registry.to_str().unwrap()])
            .assert()
            .success();
        let with_flag_out = stdout_text(&with_flag);
        assert_eq!(with_flag_out, with_env_out);
        assert!(!with_flag_out.contains("Next:"));
        assert!(!fixture.root.join("config").exists());

        let from_cwd = fixture
            .command()
            .env_remove("MY_SKILLS_ROOT")
            .current_dir(&fixture.registry)
            .assert()
            .success();
        let from_cwd_out = stdout_text(&from_cwd);
        assert_eq!(from_cwd_out, with_env_out);
        assert!(!from_cwd_out.contains("Next:"));
        assert!(!fixture.root.join("config").exists());
    }

    /// T-B4: Invalid `MY_SKILLS_ROOT` keeps the exact existing error line for
    /// `skills` (M2) and prints help without `NEXT_STEPS` for bare `my-skills` (M4).
    #[test]
    fn t_b4_invalid_env_root_keeps_existing_error_and_omits_next_steps_on_bare() {
        let fixture = Fixture::isolated();
        let outside = fixture.root.join("outside");
        let missing = fixture.root.join("does-not-exist");

        let skills = fixture
            .command()
            .env("MY_SKILLS_ROOT", &missing)
            .current_dir(&outside)
            .arg("skills")
            .assert()
            .code(2);
        let stderr = stderr_text(&skills);
        assert_eq!(
            stderr,
            format!(
                "error: MY_SKILLS_ROOT={} does not contain my-skills.toml\n",
                missing.display()
            )
        );

        let bare = fixture
            .command()
            .env("MY_SKILLS_ROOT", &missing)
            .current_dir(&outside)
            .assert()
            .success();
        let stdout = stdout_text(&bare);
        assert!(stdout.contains("Usage:"));
        assert!(!stdout.contains("Next:"));
        assert!(!fixture.root.join("config").exists());
    }

    /// T-B5: `--help`, `-h`, and `--version` are unchanged; bare `my-skills` starts
    /// with exact `--help` stdout; `my-skills --json` exits 2 with the same JSON
    /// key set and `code` as another usage error.
    #[test]
    fn t_b5_help_version_and_bare_json_usage_error() {
        let fixture = Fixture::isolated();
        let outside = fixture.root.join("outside");

        let help = fixture
            .command()
            .env_remove("MY_SKILLS_ROOT")
            .current_dir(&outside)
            .arg("--help")
            .assert()
            .success();
        let help_out = stdout_text(&help);
        assert!(help_out.contains("Usage:"));
        assert!(!help_out.contains("Next:"));

        let bare = fixture
            .command()
            .env_remove("MY_SKILLS_ROOT")
            .current_dir(&outside)
            .assert()
            .success();
        let bare_out = stdout_text(&bare);
        assert!(
            bare_out.starts_with(&help_out),
            "bare stdout did not start with exact --help stdout"
        );

        let short_help = fixture
            .command()
            .env_remove("MY_SKILLS_ROOT")
            .current_dir(&outside)
            .arg("-h")
            .assert()
            .success();
        let short_help_out = stdout_text(&short_help);
        assert!(short_help_out.contains("Usage:"));
        assert!(!short_help_out.contains("Next:"));

        let version = fixture
            .command()
            .env_remove("MY_SKILLS_ROOT")
            .current_dir(&outside)
            .arg("--version")
            .assert()
            .success();
        let version_out = stdout_text(&version);
        assert_eq!(
            version_out.trim(),
            format!("my-skills {}", env!("CARGO_PKG_VERSION"))
        );

        let bare_json = fixture
            .command()
            .env_remove("MY_SKILLS_ROOT")
            .current_dir(&outside)
            .arg("--json")
            .assert()
            .code(2);
        let payload: Value = serde_json::from_slice(&bare_json.get_output().stdout).unwrap();

        let other_usage_json = fixture
            .command()
            .env_remove("MY_SKILLS_ROOT")
            .current_dir(&outside)
            .args(["--json", "nosuchcmd"])
            .assert()
            .code(2);
        let reference: Value =
            serde_json::from_slice(&other_usage_json.get_output().stdout).unwrap();

        assert_eq!(object_keys(&payload), object_keys(&reference));
        assert_eq!(
            object_keys(&payload["error"]),
            object_keys(&reference["error"])
        );
        assert_eq!(payload["error"]["code"], reference["error"]["code"]);
    }

    fn repo_file(rel: &str) -> String {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
        fs::read_to_string(&path).unwrap().replace("\r\n", "\n")
    }

    /// T-B6: The guidance block in `scripts/install.sh` and `scripts/install.ps1`
    /// matches `NEXT_STEPS` character-for-character (ignoring trailing line spaces).
    #[test]
    fn t_b6_install_scripts_match_next_steps_exactly() {
        let expected_lines = normalized_lines(NEXT_STEPS);

        let sh = repo_file("scripts/install.sh");
        let sh_start = sh
            .find("printf '\\n%s\\n' 'Next:")
            .expect("missing single-quoted Next: block in scripts/install.sh");
        let sh_after_open = &sh[sh_start + "printf '\\n%s\\n' '".len()..];
        let sh_end = sh_after_open
            .find('\'')
            .expect("unterminated single-quoted Next: block in scripts/install.sh");
        let sh_block = &sh_after_open[..sh_end];
        assert_eq!(normalized_lines(sh_block), expected_lines);

        let ps1 = repo_file("scripts/install.ps1");
        let ps1_start = ps1
            .find("Write-Output @'\nNext:")
            .expect("missing @'...'@ Next: block in scripts/install.ps1");
        let ps1_after_open = &ps1[ps1_start + "Write-Output @'\n".len()..];
        let ps1_end = ps1_after_open
            .find("\n'@")
            .expect("unterminated @'...'@ Next: block in scripts/install.ps1");
        let ps1_block = &ps1_after_open[..ps1_end];
        assert_eq!(normalized_lines(ps1_block), expected_lines);
    }

    /// T-B7: Static checks on `scripts/install.sh` and `scripts/install.ps1`:
    /// - `"$HOME/..."` is printed literally (single quotes in sh, `@'...'@` in ps1)
    /// - the block comes after `Installed` and PATH output
    /// - in `install.ps1`, the block is after the end of `try`/`finally`, preceded
    ///   by a blank-line `Write-Output ''`, with no output statement after it
    /// - in `install.sh`, the `printf '\n%s\n'` block is after `trap` and is the
    ///   last output statement.
    #[test]
    fn t_b7_install_scripts_static_placement_and_quoting() {
        let sh = repo_file("scripts/install.sh");
        let sh_installed = sh.find("printf 'Installed %s\\n'").unwrap();
        let sh_path = sh.find("Add %s to PATH").unwrap();
        let sh_block = sh.find("printf '\\n%s\\n' 'Next:").unwrap();
        assert!(sh_installed < sh_path && sh_path < sh_block);
        let sh_trap_last = sh.rfind("trap ").unwrap();
        assert!(sh_trap_last < sh_block);
        assert!(sh[sh_block..].contains("\"$HOME/my-agent-skills\""));
        let sh_after_open = &sh[sh_block + "printf '\\n%s\\n' '".len()..];
        let sh_close = sh_after_open.find('\'').unwrap();
        let sh_tail = sh_after_open[sh_close + 1..].trim();
        assert!(
            sh_tail.is_empty(),
            "expected no statements after Next: block in install.sh, found: {sh_tail:?}"
        );

        let ps1 = repo_file("scripts/install.ps1");
        let ps1_finally = ps1.find("} finally {").unwrap();
        let ps1_finally_end = ps1[ps1_finally..]
            .find("\n}\n")
            .map(|offset| ps1_finally + offset + "\n}\n".len())
            .expect("missing closing brace for finally block in scripts/install.ps1");
        let ps1_installed = ps1.find("Write-Output \"Installed $destination\"").unwrap();
        let ps1_path = ps1.find("Add $installDir to PATH").unwrap();
        let ps1_blank_and_block = ps1.find("Write-Output ''\nWrite-Output @'\nNext:").expect(
            "expected blank-line Write-Output '' immediately before Write-Output @' in install.ps1",
        );
        assert!(
            ps1_finally_end <= ps1_installed
                && ps1_installed < ps1_path
                && ps1_path < ps1_blank_and_block
        );
        assert!(ps1[ps1_blank_and_block..].contains("\"$HOME/my-agent-skills\""));
        let ps1_after_open =
            &ps1[ps1_blank_and_block + "Write-Output ''\nWrite-Output @'\n".len()..];
        let ps1_close = ps1_after_open.find("\n'@").unwrap();
        let ps1_tail = ps1_after_open[ps1_close + "\n'@".len()..].trim();
        assert!(
            ps1_tail.is_empty(),
            "expected no statements after Next: block in install.ps1, found: {ps1_tail:?}"
        );
    }
}

mod agent_ux_p1_int {
    //! Integration-level checks for the wave 14 `--root` contract in `cli::run`.

    use crate::common::Fixture;
    use predicates::str::contains;
    use serde_json::Value;

    #[test]
    fn a_wrong_root_is_a_usage_error_for_every_command() {
        let fixture = Fixture::repo();
        fixture.write_manifest("");
        let missing = fixture.root.join("not-a-registry");
        // data-path never resolves the registry; it must still reject the flag.
        for args in [&["data-path", "alpha"][..], &["skills"], &["sync"]] {
            fixture
                .command()
                .args(args)
                .arg("--root")
                .arg(&missing)
                .assert()
                .code(2)
                .stderr(contains("does not contain my-skills.toml"));
        }
        let output = fixture
            .command()
            .args(["skills", "--json", "--root"])
            .arg(&missing)
            .assert()
            .code(2);
        let error: Value = serde_json::from_slice(&output.get_output().stdout).unwrap();
        assert_eq!(error["error"]["code"], "usage");
    }

    #[test]
    fn root_cannot_be_combined_with_set_root_or_init_registry() {
        let fixture = Fixture::repo();
        fixture.write_manifest("");
        let cache = fixture.root.join("config/my-skills/root");
        for args in [&["set-root"][..], &["init-registry", "--dry-run"]] {
            fixture
                .command()
                .args(args)
                .arg("--root")
                .arg(&fixture.registry)
                .assert()
                .code(2)
                .stderr(contains("--root cannot be combined"));
            assert!(!cache.exists(), "{args:?} wrote the root cache");
        }
    }
}

mod agent_ux_output {
    use assert_cmd::Command;
    use std::fs;

    use crate::common::{Fixture, MANIFEST};
    use crate::common::{stdout, stdout_json};
    use my_skills::platform::paths;
    use predicates::prelude::PredicateBooleanExt;
    use predicates::str::contains;
    #[cfg(windows)]
    use serde_json::Value;

    fn registry(two_hosts: bool) -> Fixture {
        let fixture = Fixture::isolated();
        let mut manifest = format!("{MANIFEST}\n[defaults]\ninstall_mode = \"copy\"\n");
        for (host, enabled) in [("claude", two_hosts), ("codex", true), ("agy", false)] {
            let path = fixture.root.join("hosts").join(host);
            manifest.push_str(&format!(
                "\n[targets.{host}]\nenabled = {enabled}\npath = '{}'\n",
                path.display()
            ));
        }
        manifest.push_str("\n[skills.example]\nenabled = true\n");
        fixture.write("my-skills.toml", &manifest);
        write_skill(&fixture, "A valid example skill.", "Example body.");
        fixture
    }

    fn write_skill(fixture: &Fixture, description: &str, body: &str) {
        fixture.write(
            "skills/example/SKILL.md",
            &format!("---\nname: example\ndescription: {description}\n---\n\n{body}\n"),
        );
    }

    fn dry_run(fixture: &Fixture, command: &str) -> Command {
        let mut cmd = fixture.command();
        cmd.args([command, "--dry-run"]);
        cmd
    }

    #[test]
    fn skills_json_emits_utf8_descriptions() {
        let fixture = registry(false);
        let description = "한국어 학습 예제 🚀";
        write_skill(&fixture, description, "Example body.");
        let out = fixture
            .command()
            .args(["skills", "--json"])
            .assert()
            .success();
        assert!(stdout(&out).contains(description));
        assert!(!stdout(&out).contains(r"\ud55c"));
        assert!(!stdout(&out).contains(r"\ud83d"));
        assert_eq!(stdout_json(&out)["skills"][0]["description"], description);
    }

    #[cfg(windows)]
    #[test]
    fn windows_displays_registry_doctor_and_plan_paths_consistently() {
        fn check_path(value: &Value) {
            let path = value.as_str().unwrap();
            assert!(
                !path.contains(r"\\?\") && !path.contains('/') && path.contains('\\'),
                "{path}"
            );
        }

        let fixture = registry(false);
        fixture.init();
        fixture.git(&["config", "core.hooksPath", "custom/hooks"]);

        let report = stdout_json(
            &fixture
                .command()
                .args(["doctor", "--no-update-check", "--json"])
                .assert()
                .success(),
        );
        for value in [
            &report["registry"]["path"],
            &report["state_path"],
            &report["data_root"],
            &report["hooks"]["path"],
        ] {
            check_path(value);
        }
        assert!(
            report["hooks"]["path"]
                .as_str()
                .unwrap()
                .ends_with(r"\custom\hooks")
        );
        for host in report["hosts"].as_array().unwrap() {
            check_path(&host["path"]);
        }

        let value = stdout_json(&dry_run(&fixture, "sync").arg("--json").assert().success());
        check_path(&value["registry"]["path"]);
        let action = &value["actions"][0];
        check_path(&action["source"]);
        check_path(&action["destination"]);

        let active = fixture.root.join("active");
        fs::create_dir(&active).unwrap();
        fs::write(active.join("my-skills.toml"), MANIFEST).unwrap();
        fixture.cache(&active);
        let state = fixture.root.join("state/my-skills/state.json");
        fs::create_dir_all(state.parent().unwrap()).unwrap();
        fs::write(&state, "{invalid").unwrap();

        for args in [
            &["doctor", "--no-update-check"][..],
            &["doctor", "--no-update-check", "--json"][..],
        ] {
            let out = fixture
                .command()
                .env_remove("MY_SKILLS_ROOT")
                .args(args)
                .assert()
                .success();
            if args.contains(&"--json") {
                let report = stdout_json(&out);
                let warning = report["registry"]["warning"].as_str().unwrap();
                assert!(warning.contains(&paths::display_path(&active)), "{warning}");
                let warning = report["connections"]["warnings"][0].as_str().unwrap();
                assert!(warning.contains(&paths::display_path(&state)), "{warning}");
            } else {
                let text = stdout(&out);
                assert!(!text.contains(r"\\?\"), "{text}");
                assert!(text.contains(&paths::display_path(&active)), "{text}");
                assert!(text.contains(&paths::display_path(&state)), "{text}");
            }
        }
    }

    #[test]
    fn invalid_skill_fails_sync_dry_run_with_machine_and_human_diagnostics() {
        let fixture = registry(false);
        fs::remove_file(fixture.registry.join("skills/example/SKILL.md")).unwrap();
        let out = dry_run(&fixture, "sync")
            .assert()
            .code(1)
            .stdout(contains("[BLOCKED] example: validation failed"))
            .stdout(contains("SKILL.md not found"));
        assert_eq!(
            stdout(&out).lines().next().unwrap(),
            "BLOCKED: 1 item(s) would be refused (validation 1, plan 0)"
        );

        let out = dry_run(&fixture, "sync")
            .arg("--json")
            .assert()
            .code(1)
            .stderr(contains("SKILL.md not found"))
            .stderr(contains("No files were changed"));
        let value = stdout_json(&out);
        assert_eq!(value["dry_run"], true);
        assert_eq!(value["blocked"], true);
        assert_eq!(value["applicable"], false);
        let validation = value["validation"].as_array().unwrap();
        let check = validation
            .iter()
            .find(|check| check["host"].is_null())
            .unwrap();
        assert_eq!(check["skill"], "example");
        assert_eq!(check["ok"], false);
        assert_eq!(check["errors"], serde_json::json!(["SKILL.md not found"]));
        assert_eq!(check["warnings"], serde_json::json!([]));
        assert_eq!(value["block_reasons"][0]["kind"], "validation");
        assert!(value["block_reasons"][0]["host"].is_null());
        assert!(!fixture.root.join("hosts").exists());
    }

    #[test]
    fn unmanaged_destination_is_reported_by_previews_and_enforced_by_real_runs() {
        let fixture = registry(false);
        let destination = fixture.root.join("hosts/codex/example");
        fs::create_dir_all(&destination).unwrap();
        let original = destination.join("SKILL.md");
        fs::write(&original, "Local skill.").unwrap();
        let out = dry_run(&fixture, "sync").assert().success();
        assert_eq!(
            stdout(&out).lines().next().unwrap(),
            "BLOCKED: 1 item(s) would be refused (validation 0, plan 1)"
        );
        let value = stdout_json(&dry_run(&fixture, "sync").arg("--json").assert().success());
        assert_eq!(value["blocked"], true);
        assert_eq!(value["applicable"], false);
        assert_eq!(value["block_reasons"][0]["kind"], "block_conflict");
        assert_eq!(value["block_reasons"][0]["skill"], "example");
        assert_eq!(value["block_reasons"][0]["host"], "codex");
        fixture
            .command()
            .arg("sync")
            .assert()
            .code(1)
            .stdout(contains("BLOCKED"));
        assert_eq!(fs::read_to_string(&original).unwrap(), "Local skill.");
        assert!(!fixture.root.join("state/my-skills/state.json").exists());
    }

    #[test]
    fn drifted_copy_is_reported_without_turning_preview_into_an_exit_code_gate() {
        let fixture = registry(false);
        fixture.command().arg("sync").assert().success();
        let copy = fixture.root.join("hosts/codex/example/SKILL.md");
        fs::write(&copy, "Locally changed.").unwrap();

        let out = dry_run(&fixture, "sync").assert().success();
        assert_eq!(
            stdout(&out).lines().next().unwrap(),
            "BLOCKED: 1 item(s) would be refused (validation 0, plan 1)"
        );
        let value = stdout_json(&dry_run(&fixture, "sync").arg("--json").assert().success());
        assert_eq!(value["blocked"], true);
        assert_eq!(value["applicable"], false);
        assert_eq!(value["actions"][0]["action"], "BLOCK_DRIFT");
        assert_eq!(value["block_reasons"][0]["kind"], "block_drift");
        assert_eq!(value["block_reasons"][0]["host"], "codex");
        fixture
            .command()
            .arg("sync")
            .assert()
            .code(1)
            .stdout(contains("BLOCKED: example -> codex"));
        assert_eq!(fs::read_to_string(copy).unwrap(), "Locally changed.");
    }

    #[test]
    fn clean_previews_show_whether_multi_host_confirmation_is_needed() {
        let fixture = registry(true);
        let value = stdout_json(&dry_run(&fixture, "sync").arg("--json").assert().success());
        assert_eq!(value["blocked"], false);
        assert_eq!(value["applicable"], false);
        assert_eq!(value["block_reasons"][0]["kind"], "needs_yes");
        assert_eq!(value["registry"]["source"], "env");
        assert_eq!(
            value["registry"]["path"],
            paths::display_path(&fixture.registry)
        );
        assert_eq!(value["actions"].as_array().unwrap().len(), 2);
        fixture
            .command()
            .arg("sync")
            .assert()
            .code(2)
            .stderr(contains("writes to multiple hosts"));

        let value = stdout_json(
            &dry_run(&fixture, "sync")
                .args(["--json", "--yes"])
                .assert()
                .success(),
        );
        assert_eq!(value["blocked"], false);
        assert_eq!(value["applicable"], true);
        assert!(value.get("audit").is_none());
        assert!(value["block_reasons"].as_array().unwrap().is_empty());
        assert!(!fixture.root.join("hosts").exists());

        fixture.command().args(["sync", "--yes"]).assert().success();
        let value = stdout_json(&dry_run(&fixture, "sync").arg("--json").assert().success());
        assert_eq!(value["applicable"], true, "no writes need confirmation");
    }

    #[test]
    fn sync_json_applies_by_default() {
        let fixture = registry(false);
        let out = fixture
            .command()
            .args(["sync", "--json"])
            .assert()
            .success();
        assert_eq!(stdout_json(&out)["actions"][0]["result"], "created");
        assert!(fixture.root.join("hosts/codex/example/SKILL.md").is_file());
    }

    #[test]
    fn sync_check_does_not_cache_a_registry_discovered_from_cwd() {
        let fixture = registry(false);
        fixture
            .command()
            .env_remove("MY_SKILLS_ROOT")
            .args(["sync", "--check"])
            .assert()
            .code(1)
            .stdout(contains("MISSING"));
        assert!(!fixture.root.join("config").exists());
    }

    fn with_different_active_registry() -> Fixture {
        let fixture = registry(false);
        let active = fixture.root.join("active");
        fs::create_dir(&active).unwrap();
        fs::write(active.join("my-skills.toml"), MANIFEST).unwrap();
        fixture.cache(&active);
        fixture
    }

    #[test]
    fn inactive_registry_blocks_sync_json_plan() {
        let fixture = with_different_active_registry();
        let real = fixture
            .command()
            .env_remove("MY_SKILLS_ROOT")
            .args(["sync", "--yes"])
            .assert()
            .code(2)
            .stderr(contains("refusing to write to cwd registry"));
        let stderr = std::str::from_utf8(&real.get_output().stderr).unwrap();
        let message = stderr.trim().strip_prefix("error: ").unwrap();
        let out = dry_run(&fixture, "sync")
            .env_remove("MY_SKILLS_ROOT")
            .args(["--json", "--yes"])
            .assert()
            .success();
        let plan = stdout_json(&out);
        assert_eq!(plan["blocked"], true, "sync: {plan}");
        assert_eq!(plan["applicable"], false);
        assert_eq!(
            plan["block_reasons"],
            serde_json::json!([{
                "skill": null,
                "host": null,
                "kind": "inactive_registry",
                "message": message,
            }])
        );
        assert_eq!(plan["actions"][0]["action"], "CREATE");
        assert!(!fixture.root.join("hosts").exists());
    }

    #[test]
    fn inactive_registry_refusal_starts_each_text_plan() {
        let fixture = with_different_active_registry();
        let mut first_lines = Vec::new();
        for args in [
            &["sync", "--dry-run"][..],
            &["sync", "--mode", "link", "--dry-run"][..],
        ] {
            let out = fixture
                .command()
                .env_remove("MY_SKILLS_ROOT")
                .args(args)
                .assert()
                .success()
                .stdout(contains("planned actions"));
            first_lines.push(stdout(&out).lines().next().unwrap().to_owned());
        }
        assert!(
            first_lines
                .iter()
                .all(|line| line.starts_with("BLOCKED: refusing to write to cwd registry ")),
            "{first_lines:?}"
        );

        fs::remove_file(fixture.registry.join("skills/example/SKILL.md")).unwrap();
        let out = dry_run(&fixture, "sync")
            .env_remove("MY_SKILLS_ROOT")
            .assert()
            .code(1);
        let mut lines = stdout(&out).lines();
        assert_eq!(lines.next().unwrap(), first_lines[0]);
        assert_eq!(
            lines.next().unwrap(),
            "BLOCKED: 1 item(s) would be refused (validation 1, plan 0)"
        );
    }

    #[test]
    fn sync_check_wins_over_dry_run() {
        let fixture = registry(false);
        let check = fixture.command().args(["sync", "--check"]).assert().code(1);
        let combined = fixture
            .command()
            .args(["sync", "--check", "--dry-run"])
            .assert()
            .code(1);
        assert_eq!(stdout(&combined), stdout(&check));
        assert!(stdout(&combined).contains("MISSING"));
    }

    #[test]
    fn sync_json_conflicts_with_check_even_with_dry_run() {
        let fixture = registry(false);
        for extra in [&["--dry-run"][..], &[][..]] {
            fixture
                .command()
                .args(["sync", "--check", "--json"])
                .args(extra)
                .assert()
                .code(2)
                .stderr(contains(
                    "sync --json is not supported with --check; use --dry-run --json",
                ));
        }
    }

    #[test]
    fn connect_usage_errors_leave_empty_config_home_alone() {
        let mut cached = Vec::new();
        for (args, error) in [
            (&["sync", "absent", "--json"][..], "unknown skill"),
            (
                &["sync", "--check", "--json"][..],
                "not supported with --check",
            ),
            (&["sync", "absent"][..], "unknown skill"),
            (&["sync", "example", "--host", "absent"][..], "unknown host"),
            (&["bootstrap", "--host", "absent"][..], "unknown host"),
        ] {
            let fixture = registry(false);
            fixture
                .command()
                .env_remove("MY_SKILLS_ROOT")
                .args(args)
                .assert()
                .code(2)
                .stderr(contains(error));
            if fixture.root.join("config").exists() {
                cached.push(args);
            }
        }
        assert!(
            cached.is_empty(),
            "usage errors created a cache: {cached:?}"
        );
    }

    #[test]
    fn multi_host_confirmation_is_the_last_line_of_text_plans() {
        let fixture = registry(true);
        let expected =
            "Re-run with --yes to apply: this command writes to multiple hosts (claude, codex).";
        let mut last_lines = Vec::new();
        for args in [
            &["sync", "--dry-run"][..],
            &["sync", "--mode", "link", "--dry-run"][..],
        ] {
            let out = fixture
                .command()
                .env_remove("MY_SKILLS_ROOT")
                .args(args)
                .assert()
                .success()
                .stdout(contains("as the active registry"));
            assert!(!stdout(&out).starts_with("BLOCKED:"));
            last_lines.push(stdout(&out).lines().last().unwrap().to_owned());
            fixture
                .command()
                .args(args)
                .arg("--yes")
                .assert()
                .success()
                .stdout(contains(expected).not());
        }
        assert_eq!(last_lines, [expected; 2]);

        fixture.command().args(["sync", "--yes"]).assert().success();
        let noop = dry_run(&fixture, "sync").assert().success();
        assert!(!stdout(&noop).contains(expected));
    }
}
