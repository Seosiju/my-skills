use assert_cmd::Command;
use predicates::str::contains;

#[test]
fn version_prints_crate_version() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path();
    Command::cargo_bin("my-skills")
        .unwrap()
        .current_dir(base)
        .env("HOME", base.join("home"))
        .env("USERPROFILE", base.join("home"))
        .env("LOCALAPPDATA", base.join("local-app-data"))
        .env("APPDATA", base.join("appdata"))
        .env("XDG_STATE_HOME", base.join("state"))
        .env("XDG_DATA_HOME", base.join("data"))
        .env("XDG_CONFIG_HOME", base.join("config"))
        .env_remove("MY_SKILLS_ROOT")
        .arg("--version")
        .assert()
        .success()
        .stdout(contains(env!("CARGO_PKG_VERSION")));
}
