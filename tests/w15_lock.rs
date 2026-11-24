mod support_h;

use std::fs::{self, File, OpenOptions};
use std::process::{Child, Stdio};

use assert_cmd::assert::OutputAssertExt;
use fs4::FileExt;
use my_skills::drive::{hash::hash_directory, state::State};
use predicates::prelude::predicate;
use support_h::{Fixture, skill};

fn repo() -> Fixture {
    let fixture = Fixture::repo();
    fixture.write_manifest("[defaults]\ninstall_mode = 'copy'\n[skills.alpha]\n[skills.beta]\n");
    for name in ["alpha", "beta"] {
        let source = skill(
            &fixture.registry.join("skills"),
            name,
            "A synthetic test skill.",
            "Test body.",
        );
        fs::create_dir(source.join("assets")).unwrap();
        fs::write(source.join("assets/data.txt"), name.repeat(128 * 1024)).unwrap();
    }
    fixture
}

fn launch(fixture: &Fixture, name: &str) -> Child {
    fixture
        .process(env!("CARGO_BIN_EXE_my-skills"))
        .args(["install", name, "--host", "claude", "--yes"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}

fn assert_installed(fixture: &Fixture, names: &[&str]) {
    let state = State::load(Some(&fixture.state())).unwrap();
    assert_eq!(state.records().count(), names.len());
    let mut entries = fs::read_dir(fixture.host("claude"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect::<Vec<_>>();
    entries.sort();
    assert_eq!(entries, names);
    for name in names {
        let record = state.get(name, "claude").unwrap();
        assert_eq!(
            record.mode,
            my_skills::registry::manifest::InstallMode::Copy
        );
        let digest = hash_directory(&fixture.canonical(name)).unwrap();
        assert_eq!(record.source_hash, digest);
        assert_eq!(record.installed_hash, digest);
        assert_eq!(
            hash_directory(&fixture.host("claude").join(name)).unwrap(),
            digest
        );
    }
}

#[test]
fn concurrent_installs_to_the_same_target_both_succeed() {
    let fixture = repo();
    let a = launch(&fixture, "alpha");
    let b = launch(&fixture, "alpha");
    a.wait_with_output().unwrap().assert().success();
    b.wait_with_output().unwrap().assert().success();
    assert_installed(&fixture, &["alpha"]);
    assert!(fixture.state().with_file_name("state.json.lock").is_file());
}

#[test]
fn concurrent_installs_to_different_targets_keep_both_records() {
    for _ in 0..10 {
        let fixture = repo();
        let a = launch(&fixture, "alpha");
        let b = launch(&fixture, "beta");
        a.wait_with_output().unwrap().assert().success();
        b.wait_with_output().unwrap().assert().success();
        assert_installed(&fixture, &["alpha", "beta"]);
    }
}

fn hold_lock(fixture: &Fixture) -> File {
    let lock = fixture.state().with_file_name("state.json.lock");
    fs::create_dir_all(lock.parent().unwrap()).unwrap();
    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock)
        .unwrap();
    FileExt::lock(&file).unwrap();
    file
}

#[test]
fn busy_writers_leave_state_and_hosts_unchanged() {
    let fixture = repo();
    fixture
        .command()
        .args(["install", "alpha", "--host", "claude", "--yes"])
        .assert()
        .success();
    let state = fs::read(fixture.state()).unwrap();
    let lock = hold_lock(&fixture);
    for args in [
        &["install", "beta", "--host", "claude", "--yes"][..],
        &["uninstall", "alpha", "--host", "claude", "--yes"][..],
    ] {
        fixture
            .command()
            .env("MY_SKILLS_STATE_LOCK_TIMEOUT_MS", "5")
            .args(args)
            .assert()
            .code(2)
            .stderr(predicate::str::starts_with("error: state is busy: "))
            .stderr(predicate::str::contains("state.json.lock"));
    }
    fixture
        .command()
        .env("MY_SKILLS_STATE_LOCK_TIMEOUT_MS", "5")
        .args(["hooks", "run", "post-merge", "0"])
        .assert()
        .success()
        .stderr(predicate::str::starts_with("error: state is busy: "))
        .stderr(predicate::str::contains(
            "warning: post-merge could not sync the registry",
        ));
    assert_eq!(state, fs::read(fixture.state()).unwrap());
    assert_installed(&fixture, &["alpha"]);
    drop(lock);
}

#[test]
fn readers_do_not_wait_for_the_state_lock() {
    let fixture = repo();
    fixture
        .command()
        .args(["install", "alpha", "--host", "claude", "--yes"])
        .assert()
        .success();
    let state = fs::read(fixture.state()).unwrap();
    let lock = hold_lock(&fixture);
    let status = support_h::json(
        fixture
            .command()
            .env("MY_SKILLS_STATE_LOCK_TIMEOUT_MS", "0")
            .args(["status", "--json"]),
    );
    assert!(
        status.as_array().unwrap().iter().any(|row| {
            row["skill"] == "alpha" && row["host"] == "claude" && row["status"] == "FRESH"
        }),
        "{status}"
    );
    let preview = support_h::json(
        fixture
            .command()
            .env("MY_SKILLS_STATE_LOCK_TIMEOUT_MS", "0")
            .args(["sync", "--host", "claude", "--dry-run", "--json"]),
    );
    assert_eq!(preview["blocked"], false);
    assert!(
        preview["actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| { row["skill"] == "beta" && row["action"] == "CREATE" })
    );
    fixture
        .command()
        .env("MY_SKILLS_STATE_LOCK_TIMEOUT_MS", "0")
        .args(["sync", "alpha", "--host", "claude", "--check"])
        .assert()
        .success()
        .stderr("");
    assert_eq!(state, fs::read(fixture.state()).unwrap());
    assert_installed(&fixture, &["alpha"]);
    drop(lock);
}

#[test]
fn share_takes_the_manifest_lock_first_and_reports_busy_as_json() {
    let fixture = repo();
    let copy = skill(
        &fixture.host("claude"),
        "gamma",
        "A local test skill.",
        "Test body.",
    );
    let manifest_before = fs::read(fixture.manifest()).unwrap();
    let state_lock = hold_lock(&fixture);
    let manifest_lock = fixture.manifest().with_extension("toml.edit-lock");
    fs::write(&manifest_lock, "").unwrap();
    let attempt_share = || {
        let mut command = fixture.command();
        command
            .env_remove("MY_SKILLS_ROOT")
            .env("MY_SKILLS_STATE_LOCK_TIMEOUT_MS", "0")
            .args(["share", "gamma", "--from", "claude", "--enable", "--json"]);
        command
    };

    attempt_share()
        .assert()
        .code(2)
        .stderr(predicate::str::contains("my-skills.toml.edit-lock"));

    fixture
        .command()
        .args(["share", "--from", "claude", "--plan", "--json"])
        .assert()
        .success()
        .stderr("");
    fs::remove_file(&manifest_lock).unwrap();

    let busy = attempt_share()
        .assert()
        .code(2)
        .stderr(predicate::str::starts_with("error: state is busy: "));
    let payload: serde_json::Value = serde_json::from_slice(&busy.get_output().stdout).unwrap();
    assert_eq!(payload["error"]["code"], "state_busy");
    assert_eq!(fs::read(fixture.manifest()).unwrap(), manifest_before);
    assert!(copy.is_dir());
    assert!(!fixture.state().exists());
    assert!(!fixture.canonical("gamma").exists());
    assert!(!fixture.root.join("config/my-skills/root").exists());
    assert!(!manifest_lock.exists());
    drop(state_lock);
}

#[test]
fn one_install_copies_to_two_hosts_and_records_matching_hashes() {
    let fixture = repo();
    fixture
        .command()
        .args(["install", "alpha", "--yes"])
        .assert()
        .success();
    let state = State::load(Some(&fixture.state())).unwrap();
    assert_eq!(state.records().count(), 2);
    let hash = hash_directory(&fixture.canonical("alpha")).unwrap();
    for host in ["claude", "codex"] {
        let record = state.get("alpha", host).unwrap();
        assert_eq!(record.source_hash, hash);
        assert_eq!(record.installed_hash, hash);
        assert_eq!(
            hash_directory(&fixture.host(host).join("alpha")).unwrap(),
            hash
        );
        assert_eq!(fs::read_dir(fixture.host(host)).unwrap().count(), 1);
    }
}

#[test]
fn a_copy_from_a_linked_canonical_directory_tracks_the_target_bytes() {
    let fixture = repo();
    let target = skill(
        &fixture.root,
        "source",
        "A synthetic test skill.",
        "Test body.",
    );
    let canonical = fixture.canonical("beta");
    // Keep the original canonical entry outside the manifest's skills root.
    fs::rename(&canonical, fixture.root.join("original-beta")).unwrap();
    fs::write(
        target.join("SKILL.md"),
        "---\nname: beta\ndescription: A synthetic test skill.\n---\n\nTest body.",
    )
    .unwrap();
    support_h::link_dir(&target, &canonical);
    fixture
        .command()
        .args(["install", "beta", "--host", "claude", "--yes"])
        .assert()
        .success();
    let status = || {
        support_h::json(fixture.command().args(["status", "--json"]))
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["skill"] == "beta" && row["host"] == "claude")
            .unwrap()["status"]
            .clone()
    };
    assert_eq!(status(), "FRESH");
    fs::write(target.join("asset.txt"), "New content.").unwrap();
    assert_eq!(status(), "STALE");
}

#[cfg(windows)]
#[test]
fn backup_cleanup_failure_keeps_the_update_applied() {
    let fixture = repo();
    fixture
        .command()
        .args(["install", "alpha", "--host", "claude", "--yes"])
        .assert()
        .success();
    let host = fixture.host("claude");
    let installed = host.join("alpha");
    let relative = "assets/data.txt";
    let original = installed.join(relative);
    let old_bytes = fs::read(&original).unwrap();
    // Deny deletion of a child of the backup without blocking the rename
    // of its top-level directory. Read-only attributes alone allow deletion.
    let user = fixture
        .process("whoami")
        .args(["/user", "/fo", "csv", "/nh"])
        .output()
        .unwrap();
    assert!(user.status.success(), "{user:?}");
    let csv = String::from_utf8(user.stdout).unwrap();
    let (_, sid) = csv.trim().rsplit_once(',').unwrap();
    let account = format!("*{}", sid.trim_matches('"'));
    let acl = |path: &std::path::Path, action: &str, rule: &str| {
        fixture
            .process("icacls")
            .arg(path)
            .args([action, rule])
            .output()
            .unwrap()
            .assert()
            .success();
    };
    acl(&original, "/deny", &format!("{account}:(DE)"));
    acl(
        original.parent().unwrap(),
        "/deny",
        &format!("{account}:(DC)"),
    );

    let source = fixture.canonical("alpha");
    fs::write(source.join(relative), "Updated data.").unwrap();
    let result = fixture
        .command()
        .args(["sync", "alpha", "--host", "claude", "--yes"])
        .output()
        .unwrap();
    let backup = fs::read_dir(&host)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(".alpha.backup-")
        });
    let protected = backup.as_ref().map_or(original, |path| path.join(relative));
    if protected.exists() {
        acl(&protected, "/remove:d", &account);
    }
    if let Some(parent) = protected.parent().filter(|parent| parent.exists()) {
        acl(parent, "/remove:d", &account);
    }
    let backup = backup.unwrap_or_else(|| panic!("expected leftover backup; got {result:?}"));
    let backed_up_file = backup.join(relative);

    use my_skills::platform::paths::display_path;
    result
        .assert()
        .success()
        .stdout(predicate::str::contains("updated: alpha -> claude"))
        .stderr(predicate::str::contains("warning: cleanup"))
        .stderr(predicate::str::contains(display_path(&backup)));
    let state = State::load(Some(&fixture.state())).unwrap();
    let record = state.get("alpha", "claude").unwrap();
    let digest = hash_directory(&source).unwrap();
    assert_eq!(record.source_hash, digest);
    assert_eq!(record.installed_hash, digest);
    assert_eq!(hash_directory(&installed).unwrap(), digest);
    assert_eq!(fs::read(backed_up_file).unwrap(), old_bytes);
}
