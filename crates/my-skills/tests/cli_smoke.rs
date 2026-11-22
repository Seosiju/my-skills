use assert_cmd::Command;
use predicates::str::contains;

#[test]
fn version_prints_crate_version() {
    Command::cargo_bin("my-skills")
        .unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout(contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn unwired_command_exits_2() {
    Command::cargo_bin("my-skills")
        .unwrap()
        .arg("status")
        .assert()
        .code(2)
        .stderr(contains("not implemented yet"));
}
