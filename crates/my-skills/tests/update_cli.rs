mod support_i;

use std::fs;

use assert_cmd::assert::OutputAssertExt;
use predicates::prelude::predicate;
use support_i::{CURRENT, Fixture, NEXT, archive_name};

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
        .command()
        .args(["update", "--check"])
        .assert()
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
        .command()
        .args(["update", "--check", "--channel", "main"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Already up to date"));

    fixture.release_list(&[("v0.4.2", false, false)]);
    fixture
        .command()
        .args(["update", "--check", "--channel", "main"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Installed version is ahead of the latest main release",
        ));
}

#[test]
fn update_returns_two_when_stable_check_cannot_resolve_releases() {
    let fixture = Fixture::new();
    fixture
        .command()
        .args(["update", "--check"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "Update status unknown: could not fetch GitHub releases",
        ));

    fixture.release_list(&[("v0.4.2", false, false), ("v1.0.0-rc.1", true, false)]);
    fixture
        .command()
        .args(["update", "--check"])
        .assert()
        .code(2)
        .stderr("Update status unknown: no stable v1 releases found\n");
}

#[test]
fn update_dry_run_prints_main_channel_downloads_and_writes_nothing() {
    let fixture = Fixture::new();
    fixture.release_list(&[
        ("v1.1.0", false, false),
        ("v1.2.0-rc.2", true, false),
        ("v1.2.0-rc.10", true, false),
    ]);
    let before = fs::read(&fixture.exe).unwrap();
    let asset_url = fixture.asset_url();
    let archive = asset_url.join(archive_name()).unwrap();
    let checksums = asset_url.join("SHA256SUMS").unwrap();
    fixture
        .command()
        .args(["update", "--channel", "main", "--dry-run"])
        .assert()
        .success()
        .stdout(format!(
            "Current: my-skills {CURRENT}\nChannel: main\nLatest:  v1.2.0-rc.10\n\
             Would download: {checksums}\nWould download: {archive}\nWould replace:  {}\n",
            fixture.exe.canonicalize().unwrap().display()
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
        .command()
        .arg("update")
        .assert()
        .success()
        .stdout(predicate::str::contains(format!(
            "Updated: my-skills {NEXT}"
        )))
        .stderr("");
    fixture
        .command()
        .arg("--version")
        .assert()
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
        .command()
        .arg("--version")
        .assert()
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
        .command()
        .arg("update")
        .assert()
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
        .command()
        .arg("update")
        .assert()
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
        .command()
        .arg("update")
        .assert()
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
