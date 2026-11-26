#[allow(dead_code)]
mod support_g;

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use assert_cmd::prelude::*;
use my_skills::commands::first_run::NEXT_STEPS;
use serde_json::Value;
use support_g::Fixture;

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
    let outside = fixture.base.join("outside");
    let missing = fixture.base.join("does-not-exist");

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
    let payload: Value = serde_json::from_slice(&unconfigured_json.get_output().stdout).unwrap();

    let other_config_json = fixture
        .command()
        .env("MY_SKILLS_ROOT", &missing)
        .current_dir(&outside)
        .args(["skills", "--json"])
        .assert()
        .code(2);
    let reference: Value = serde_json::from_slice(&other_config_json.get_output().stdout).unwrap();

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
    let outside = fixture.base.join("outside");

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
    let outside = fixture.base.join("outside");

    let unconfigured = fixture
        .command()
        .env_remove("MY_SKILLS_ROOT")
        .current_dir(&outside)
        .assert()
        .success();
    let unconfigured_out = stdout_text(&unconfigured);
    assert!(unconfigured_out.contains("Usage:"));
    assert!(unconfigured_out.ends_with(&format!("\n\n{NEXT_STEPS}\n")));
    assert!(!fixture.base.join("xdg-config").exists());

    let with_env = fixture
        .command()
        .env("MY_SKILLS_ROOT", &fixture.registry)
        .current_dir(&outside)
        .assert()
        .success();
    let with_env_out = stdout_text(&with_env);
    assert_eq!(unconfigured_out, format!("{with_env_out}\n{NEXT_STEPS}\n"));
    assert!(!with_env_out.contains("Next:"));
    assert!(!fixture.base.join("xdg-config").exists());

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
    assert!(!fixture.base.join("xdg-config").exists());

    let from_cwd = fixture
        .command()
        .env_remove("MY_SKILLS_ROOT")
        .current_dir(&fixture.registry)
        .assert()
        .success();
    let from_cwd_out = stdout_text(&from_cwd);
    assert_eq!(from_cwd_out, with_env_out);
    assert!(!from_cwd_out.contains("Next:"));
    assert!(!fixture.base.join("xdg-config").exists());
}

/// T-B4: Invalid `MY_SKILLS_ROOT` keeps the exact existing error line for
/// `skills` (M2) and prints help without `NEXT_STEPS` for bare `my-skills` (M4).
#[test]
fn t_b4_invalid_env_root_keeps_existing_error_and_omits_next_steps_on_bare() {
    let fixture = Fixture::isolated();
    let outside = fixture.base.join("outside");
    let missing = fixture.base.join("does-not-exist");

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
    assert!(!fixture.base.join("xdg-config").exists());
}

/// T-B5: `--help`, `-h`, and `--version` are unchanged; bare `my-skills` starts
/// with exact `--help` stdout; `my-skills --json` exits 2 with the same JSON
/// key set and `code` as another usage error.
#[test]
fn t_b5_help_version_and_bare_json_usage_error() {
    let fixture = Fixture::isolated();
    let outside = fixture.base.join("outside");

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
    let reference: Value = serde_json::from_slice(&other_usage_json.get_output().stdout).unwrap();

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
    let ps1_after_open = &ps1[ps1_blank_and_block + "Write-Output ''\nWrite-Output @'\n".len()..];
    let ps1_close = ps1_after_open.find("\n'@").unwrap();
    let ps1_tail = ps1_after_open[ps1_close + "\n'@".len()..].trim();
    assert!(
        ps1_tail.is_empty(),
        "expected no statements after Next: block in install.ps1, found: {ps1_tail:?}"
    );
}
