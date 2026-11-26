#[allow(dead_code)]
mod support_g;

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

fn normalized_lines(text: &str) -> Vec<&str> {
    text.replace("\r\n", "\n")
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .into_iter()
        .map(|s| s.to_owned())
        .collect::<Vec<_>>()
        .leak()
        .iter()
        .map(String::as_str)
        .collect()
}

/// T-B1: Unconfigured environment `skills` prints B2 error with exit code 2,
/// and `skills --json` emits a JSON error whose message contains `New here?`.
#[test]
fn t_b1_unconfigured_read_command_shows_b2_and_json_error() {
    let fixture = Fixture::isolated();
    let outside = fixture.base.join("outside");

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

    let json_result = fixture
        .command()
        .env_remove("MY_SKILLS_ROOT")
        .current_dir(&outside)
        .args(["skills", "--json"])
        .assert()
        .code(2);
    let payload: Value = serde_json::from_slice(&json_result.get_output().stdout).unwrap();
    let message = payload["error"]["message"]
        .as_str()
        .or_else(|| payload["message"].as_str())
        .unwrap_or_else(|| panic!("unexpected json error payload: {payload}"));
    assert!(
        message.contains("New here?"),
        "expected 'New here?' in JSON error message: {payload}"
    );
}

/// T-B2: Unconfigured environment `doctor` shows B3 block and `doctor --json`
/// keeps its existing shape.
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
    assert!(report["registry"]["path"].is_null());
    assert!(report["registry"]["source"].is_null());
    assert!(report["registry"]["error"].is_null());
    assert_eq!(report["manifest"]["status"], "not_checked");
}

/// T-B3: Bare `my-skills` prints help + NEXT_STEPS when unconfigured, help only
/// when a registry is configured via `MY_SKILLS_ROOT` or `--root`, and never
/// writes the root cache.
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
}

/// T-B4: Invalid `MY_SKILLS_ROOT` keeps existing error message for `skills`
/// (M2) and prints help without `NEXT_STEPS` for bare `my-skills` (M4).
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
    assert!(
        stderr.contains("MY_SKILLS_ROOT=") && stderr.contains("does not contain my-skills.toml"),
        "unexpected stderr: {stderr}"
    );
    assert!(!stderr.contains("Next:"));

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

/// T-B5: `--help` and `--version` are unchanged, and `my-skills --json` exits 2
/// with a JSON usage error.
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
    assert!(
        payload["error"].is_object() || payload["message"].is_string(),
        "expected JSON usage error: {payload}"
    );
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
/// - the block is not inside `finally` or `trap`.
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

    let ps1 = repo_file("scripts/install.ps1");
    let ps1_finally = ps1.find("} finally {").unwrap();
    let ps1_installed = ps1.find("Write-Output \"Installed $destination\"").unwrap();
    let ps1_path = ps1.find("Add $installDir to PATH").unwrap();
    let ps1_block = ps1.find("Write-Output @'\nNext:").unwrap();
    assert!(ps1_finally < ps1_installed && ps1_installed < ps1_path && ps1_path < ps1_block);
    assert!(ps1[ps1_block..].contains("\"$HOME/my-agent-skills\""));
}
