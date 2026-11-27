//! Integration-level checks for the wave 14 `--root` contract in `cli::run`.

mod support_h;

use predicates::str::contains;
use serde_json::Value;
use support_h::Fixture;

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
