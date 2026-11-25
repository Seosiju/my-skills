mod support_h;

use std::fs;
use std::path::Path;

use assert_cmd::Command;
use my_skills::drive::link::{self, EntryKind};
use my_skills::drive::state::State;
use my_skills::platform::paths::display_path;
use my_skills::registry::manifest::InstallMode;
use predicates::str::contains;
use serde_json::{Value, json};
use support_h::{Fixture, skill};

fn write_registry(root: &Path, host: &Path, mode: &str, registered: Option<(&str, bool)>) {
    fs::create_dir_all(root.join("skills")).unwrap();
    let host = serde_json::to_string(host).unwrap();
    let mut manifest = format!(
        "schema_version = 1\n\n[defaults]\ninstall_mode = '{mode}'\n\n\
         [targets.claude]\nenabled = true\npath = {host}\n\n\
         [targets.codex]\nenabled = false\n\n[targets.agy]\nenabled = false\n"
    );
    if let Some((name, enabled)) = registered {
        manifest.push_str(&format!("\n[skills.{name}]\nenabled = {enabled}\n"));
        skill(&root.join("skills"), name, "Fixture skill.", mode);
    }
    fs::write(root.join("my-skills.toml"), manifest).unwrap();
}

fn from_cwd(fixture: &Fixture, root: &Path) -> Command {
    let mut command = fixture.command();
    command.current_dir(root).env_remove("MY_SKILLS_ROOT");
    command
}

fn with_root(fixture: &Fixture, root: &Path) -> Command {
    let mut command = from_cwd(fixture, root);
    command.env("MY_SKILLS_ROOT", root);
    command
}

fn json_output(command: &mut Command) -> Value {
    let output = command.assert().success();
    serde_json::from_slice(&output.get_output().stdout).unwrap()
}

fn registry_meta(root: &Path, source: &str) -> Value {
    json!({ "path": display_path(&root.canonicalize().unwrap()), "source": source })
}

#[test]
fn read_only_commands_and_previews_leave_the_root_cache_uncreated() {
    let fixture = Fixture::new();
    let root = &fixture.registry;
    write_registry(root, &fixture.host("claude"), "copy", Some(("alpha", true)));
    let config = fixture.root.join("config/my-skills");
    let cache = config.join("root");

    for args in [
        &["validate"][..],
        &["status"],
        &["skills"],
        &["data-path", "alpha"],
        &["data-path", "alpha", "--create"],
        &["share", "--plan", "--from", "claude"],
        &["install", "--dry-run"],
        &["sync", "--dry-run"],
    ] {
        from_cwd(&fixture, root).args(args).assert().success();
        assert!(!config.exists(), "{args:?} created the config directory");
    }

    let catalog = json_output(from_cwd(&fixture, root).args(["skills", "--json"]));
    assert_eq!(catalog["registry"], registry_meta(root, "cwd"));
    assert_eq!(catalog["skills"][0]["name"], "alpha");
    for flag in ["--plan", "--dry-run"] {
        let share = json_output(
            from_cwd(&fixture, root).args(["share", "--from", "claude", flag, "--json"]),
        );
        assert_eq!(share["registry"], registry_meta(root, "cwd"));
        assert_eq!(share["from"], "claude");
        assert_eq!(share["source"], display_path(&fixture.host("claude")));
        assert_eq!(share["candidates"], json!([]));
    }
    assert!(!cache.exists());
    assert!(!config.exists());

    from_cwd(&fixture, root)
        .args(["install", "--yes"])
        .assert()
        .success();
    assert_eq!(
        fs::read_to_string(&cache).unwrap().trim(),
        root.canonicalize().unwrap().to_str().unwrap(),
    );
    let by_cache =
        json_output(from_cwd(&fixture, &fixture.root.join("elsewhere")).args(["skills", "--json"]));
    assert_eq!(by_cache["registry"], registry_meta(root, "cache"));
    let by_env = json_output(with_root(&fixture, root).args(["skills", "--json"]));
    assert_eq!(by_env["registry"], registry_meta(root, "env"));
}

#[test]
fn a_different_cwd_registry_requires_explicit_selection_for_writes() {
    let fixture = Fixture::new();
    let a = &fixture.registry;
    let b = fixture.root.join("registry-b");
    let host = fixture.host("claude");
    write_registry(a, &host, "copy", Some(("alpha", true)));
    write_registry(&b, &host, "copy", Some(("beta", true)));
    from_cwd(&fixture, a)
        .args(["install", "--yes"])
        .assert()
        .success();

    let incoming = skill(
        &fixture.root.join("incoming"),
        "gamma",
        "Import fixture.",
        "Body.",
    );
    let a_display = display_path(&a.canonicalize().unwrap());
    let b_display = display_path(&b.canonicalize().unwrap());
    let cache = fixture.root.join("config/my-skills/root");
    let before_cache = fs::read(&cache).unwrap();
    let before_state = fs::read(fixture.state()).unwrap();
    let b_manifest = b.join("my-skills.toml");
    let before_manifest = fs::read(&b_manifest).unwrap();
    let installed = host.join("alpha/SKILL.md");
    let before_install = fs::read(&installed).unwrap();
    let beta = b.join("skills/beta/SKILL.md");
    let before_beta = fs::read(&beta).unwrap();

    for args in [
        &["install", "--yes"][..],
        &["sync", "--yes"],
        &["enable", "beta"],
        &["disable", "beta"],
        &["import", incoming.to_str().unwrap()],
        &["bootstrap"],
    ] {
        let check = from_cwd(&fixture, &b).args(args).assert().code(2);
        let stderr = String::from_utf8_lossy(&check.get_output().stderr);
        for text in [
            &a_display,
            &b_display,
            "Run from the active registry",
            "my-skills set-root <path>",
            "MY_SKILLS_ROOT=<path>",
        ] {
            assert!(stderr.contains(text), "{args:?}: {stderr}");
        }
        assert!(!stderr.contains(r"\\?\"), "{stderr}");
        assert_eq!(fs::read(&cache).unwrap(), before_cache);
        assert_eq!(fs::read(fixture.state()).unwrap(), before_state);
        assert_eq!(fs::read(&b_manifest).unwrap(), before_manifest);
        assert_eq!(fs::read(&beta).unwrap(), before_beta);
        assert_eq!(fs::read(&installed).unwrap(), before_install);
        assert_eq!(fs::read_dir(&host).unwrap().count(), 1);
        assert_eq!(fs::read_dir(b.join("skills")).unwrap().count(), 1);
    }

    let read = from_cwd(&fixture, &b).arg("skills").assert().success();
    let stderr = String::from_utf8_lossy(&read.get_output().stderr);
    assert!(
        stderr.contains(&format!("active registry is {a_display}")),
        "{stderr}"
    );
    assert!(!stderr.contains(r"\\?\"), "{stderr}");

    with_root(&fixture, &b)
        .args(["install", "--yes"])
        .assert()
        .success();
    assert_eq!(fs::read(host.join("beta/SKILL.md")).unwrap(), before_beta);
    assert_eq!(fs::read(cache).unwrap(), before_cache);
}

#[test]
fn sync_leaves_foreign_copy_orphans_and_unwanted_connections_intact() {
    let fixture = Fixture::new();
    let a = &fixture.registry;
    let b = fixture.root.join("registry-b");
    let host = fixture.host("claude");
    write_registry(a, &host, "copy", Some(("alpha", true)));
    with_root(&fixture, a)
        .args(["install", "--yes"])
        .assert()
        .success();
    let installed = host.join("alpha/SKILL.md");
    let before_install = fs::read(&installed).unwrap();
    let before_state = fs::read(fixture.state()).unwrap();

    for registered in [None, Some(("alpha", false))] {
        write_registry(&b, &host, "copy", registered);
        with_root(&fixture, &b)
            .args(["sync", "--yes"])
            .assert()
            .success();
        assert_eq!(fs::read(&installed).unwrap(), before_install);
        assert_eq!(fs::read(fixture.state()).unwrap(), before_state);
    }
}

#[test]
fn foreign_wanted_copy_is_visible_and_every_connection_plan_blocks_it() {
    let fixture = Fixture::new();
    let a = &fixture.registry;
    let b = fixture.root.join("registry-b");
    let host = fixture.host("claude");
    write_registry(a, &host, "copy", Some(("alpha", true)));
    write_registry(&b, &host, "link", Some(("alpha", true)));
    with_root(&fixture, a)
        .args(["install", "--yes"])
        .assert()
        .success();
    let installed = host.join("alpha/SKILL.md");
    let before_install = fs::read(&installed).unwrap();
    let before_state = fs::read(fixture.state()).unwrap();

    let status = json_output(with_root(&fixture, &b).args(["status", "--json"]));
    assert_eq!(status[0]["status"], "FOREIGN");
    let skills = json_output(with_root(&fixture, &b).args(["skills", "--json"]));
    assert_eq!(skills["skills"][0]["status"]["claude"], "FOREIGN");
    let source = display_path(&a.join("skills/alpha").canonicalize().unwrap());
    let reason = format!("connection belongs to another registry ({source})");
    for args in [
        &["install", "--dry-run"][..],
        &["sync", "--dry-run"],
        &["convert", "--to", "link", "--dry-run"],
        &["convert", "--to", "copy", "--dry-run"],
        &["uninstall", "alpha", "--dry-run"],
    ] {
        with_root(&fixture, &b)
            .args(args)
            .assert()
            .success()
            .stdout(contains("BLOCK_FOREIGN"))
            .stdout(contains(&reason));
    }

    let b_manifest = fs::read(b.join("my-skills.toml")).unwrap();
    let b_skill = fs::read(b.join("skills/alpha/SKILL.md")).unwrap();
    for args in [
        &["install", "--yes"][..],
        &["sync", "--yes"],
        &["convert", "--to", "link", "--yes"],
        &["share", "alpha", "--from", "claude", "--enable", "--force"],
    ] {
        with_root(&fixture, &b)
            .args(args)
            .assert()
            .code(1)
            .stdout(contains(&reason));
        assert_eq!(fs::read(&installed).unwrap(), before_install);
        assert_eq!(fs::read(fixture.state()).unwrap(), before_state);
    }
    assert_eq!(fs::read(b.join("my-skills.toml")).unwrap(), b_manifest);
    assert_eq!(fs::read(b.join("skills/alpha/SKILL.md")).unwrap(), b_skill);

    with_root(&fixture, &b)
        .args(["uninstall", "alpha", "--yes"])
        .assert()
        .code(1)
        .stdout(contains(&reason));
    assert_eq!(fs::read(&installed).unwrap(), before_install);
    assert_eq!(fs::read(fixture.state()).unwrap(), before_state);

    fs::remove_file(&installed).unwrap();
    fs::remove_dir(host.join("alpha")).unwrap();
    with_root(&fixture, &b)
        .args(["sync", "--yes"])
        .assert()
        .code(1)
        .stdout(contains(&reason));
    assert!(!host.join("alpha").exists());
    assert_eq!(fs::read(fixture.state()).unwrap(), before_state);
}

#[test]
fn sync_leaves_foreign_link_orphans_intact() {
    let fixture = Fixture::new();
    let a = &fixture.registry;
    let b = fixture.root.join("registry-b");
    let host = fixture.host("claude");
    write_registry(a, &host, "link", Some(("alpha", true)));
    write_registry(&b, &host, "link", None);
    with_root(&fixture, a)
        .args(["install", "--yes"])
        .assert()
        .success();
    let connection = host.join("alpha");
    assert!(matches!(
        link::entry_kind(&connection).unwrap(),
        EntryKind::Link(_)
    ));
    let before_target = link::link_target(&connection).unwrap();
    let before_state = fs::read(fixture.state()).unwrap();
    let state = State::load(Some(&fixture.state())).unwrap();
    assert_eq!(
        state.get("alpha", "claude").unwrap().mode,
        InstallMode::Link
    );

    with_root(&fixture, &b)
        .args(["sync", "--yes"])
        .assert()
        .success();
    assert_eq!(link::link_target(&connection).unwrap(), before_target);
    assert_eq!(fs::read(fixture.state()).unwrap(), before_state);
}

fn init_git(fixture: &Fixture, root: &Path) {
    assert!(
        fixture
            .process("git")
            .current_dir(root)
            .args(["init", "-q"])
            .status()
            .unwrap()
            .success()
    );
}

#[test]
fn hook_previews_leave_the_cache_empty_and_first_write_records_the_cwd() {
    let fixture = Fixture::new();
    let root = &fixture.registry;
    write_registry(root, &fixture.host("claude"), "copy", None);
    init_git(&fixture, root);
    let config = fixture.root.join("config/my-skills");

    for args in [
        &["hooks", "status"][..],
        &["hooks", "install", "--dry-run"],
        &["hooks", "uninstall", "--dry-run"],
    ] {
        from_cwd(&fixture, root).args(args).assert().success();
        assert!(!config.exists(), "{args:?} created the config directory");
    }
    assert!(!root.join(".git/hooks/pre-commit").exists());

    from_cwd(&fixture, root)
        .args(["hooks", "install"])
        .assert()
        .success();
    assert!(root.join(".git/hooks/pre-commit").is_file());
    assert_eq!(
        fs::read_to_string(config.join("root")).unwrap().trim(),
        root.canonicalize().unwrap().to_str().unwrap(),
    );
}

#[test]
fn hook_writes_require_an_explicit_override_in_the_inactive_cwd() {
    let fixture = Fixture::new();
    let active = &fixture.registry;
    let other = fixture.root.join("registry-b");
    let host = fixture.host("claude");
    write_registry(active, &host, "copy", None);
    write_registry(&other, &host, "copy", None);
    init_git(&fixture, &other);
    from_cwd(&fixture, active)
        .arg("set-root")
        .assert()
        .success();
    let cache = fixture.root.join("config/my-skills/root");
    let before_cache = fs::read(&cache).unwrap();
    let hook = other.join(".git/hooks/pre-commit");

    let install = from_cwd(&fixture, &other)
        .args(["hooks", "install"])
        .output()
        .unwrap();
    let installed_by_cwd = hook.exists();

    with_root(&fixture, &other)
        .args(["hooks", "install"])
        .assert()
        .success();
    let before_hook = fs::read(&hook).unwrap();
    let uninstall = from_cwd(&fixture, &other)
        .args(["hooks", "uninstall"])
        .output()
        .unwrap();
    assert_eq!(
        [install.status.code(), uninstall.status.code()],
        [Some(2), Some(2)]
    );
    for output in [install, uninstall] {
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("refusing to write to cwd registry")
        );
    }
    assert!(!installed_by_cwd);
    assert_eq!(fs::read(&hook).unwrap(), before_hook);
    assert_eq!(fs::read(cache).unwrap(), before_cache);
}

#[test]
fn sync_check_ignores_a_foreign_record_for_an_unsupported_host() {
    let fixture = Fixture::new();
    let a = &fixture.registry;
    let b = fixture.root.join("registry-b");
    let claude = fixture.host("claude");
    let codex = serde_json::to_string(&fixture.host("codex")).unwrap();
    write_registry(a, &claude, "copy", Some(("split-host", true)));
    write_registry(&b, &claude, "copy", Some(("split-host", true)));
    let manifest = b.join("my-skills.toml");
    let text = fs::read_to_string(&manifest).unwrap().replace(
        "[targets.codex]\nenabled = false",
        &format!("[targets.codex]\nenabled = true\npath = {codex}"),
    );
    fs::write(manifest, format!("{text}hosts = [\"codex\"]\n")).unwrap();
    with_root(&fixture, a)
        .args(["install", "--yes"])
        .assert()
        .success();
    let claude_skill = claude.join("split-host/SKILL.md");
    let before = fs::read(&claude_skill).unwrap();

    with_root(&fixture, &b)
        .args(["sync", "split-host", "--yes"])
        .assert()
        .success();
    with_root(&fixture, &b)
        .args(["sync", "split-host", "--check"])
        .assert()
        .success()
        .stdout(contains("claude   UNSUPPORTED"))
        .stdout(contains("codex    FRESH"));
    assert_eq!(fs::read(claude_skill).unwrap(), before);
}

#[test]
fn share_previews_skip_foreign_host_copies() {
    let fixture = Fixture::new();
    let a = &fixture.registry;
    let b = fixture.root.join("registry-b");
    let host = fixture.host("claude");
    write_registry(a, &host, "copy", Some(("alpha", true)));
    write_registry(&b, &host, "copy", None);
    with_root(&fixture, a)
        .args(["install", "--yes"])
        .assert()
        .success();
    let source = display_path(&a.join("skills/alpha").canonicalize().unwrap());
    let reason = format!("connection belongs to another registry ({source})");
    with_root(&fixture, &b)
        .args(["share", "alpha", "--from", "claude", "--enable"])
        .assert()
        .code(1)
        .stdout(contains(&reason));
    let before_state = fs::read(fixture.state()).unwrap();

    for mode in ["--plan", "--dry-run"] {
        let plan = json_output(
            with_root(&fixture, &b).args(["share", "--from", "claude", mode, "--json"]),
        );
        let candidate = &plan["candidates"][0];
        assert_eq!(candidate["name"], "alpha");
        assert_eq!(candidate["recommended"], "skip", "{candidate}");
        assert_eq!(candidate["choices"], json!(["skip"]));
        assert!(candidate["risks"].as_array().unwrap().contains(&json!({
            "severity": "error", "message": reason,
        })));
    }
    assert_eq!(fs::read(fixture.state()).unwrap(), before_state);
    assert!(!b.join("skills/alpha").exists());
}

#[cfg(windows)]
#[test]
fn invalid_manifest_diagnostic_uses_the_display_path() {
    let fixture = Fixture::new();
    let path = fixture.registry.join("my-skills.toml");
    fs::write(&path, "[invalid").unwrap();
    assert!(path.to_str().unwrap().starts_with(r"\\?\"));

    with_root(&fixture, &fixture.registry)
        .arg("skills")
        .assert()
        .code(2)
        .stderr(contains(format!(
            "invalid TOML in {}:",
            display_path(&path)
        )));
}

#[cfg(windows)]
#[test]
fn data_path_prints_native_separators_and_points_to_the_directory() {
    let fixture = Fixture::new();
    let data_home = fixture.root.join("data");
    let mixed_home = display_path(&data_home).replace('\\', "/");
    let expected = data_home.join("my-skills/data/alpha");

    for home in [data_home.to_str().unwrap(), &mixed_home] {
        let actual = fixture
            .command()
            .env("LOCALAPPDATA", home)
            .args(["data-path", "alpha", "--create"])
            .assert()
            .success();
        let output = std::str::from_utf8(&actual.get_output().stdout)
            .unwrap()
            .trim_end();
        assert_eq!(output, display_path(&expected));
        assert_eq!(
            Path::new(output).canonicalize().unwrap(),
            expected.canonicalize().unwrap()
        );
    }
}

#[test]
fn unknown_manifest_skills_and_missing_share_arguments_leave_the_cache_empty() {
    let mut wrote_cache = Vec::new();
    for (args, error) in [
        (&["enable", "unknown"][..], "unknown skill: unknown"),
        (&["disable", "unknown"], "unknown skill: unknown"),
        (
            &["share", "--from", "claude", "--enable"],
            "share requires a skill name",
        ),
    ] {
        let fixture = Fixture::new();
        let root = &fixture.registry;
        write_registry(root, &fixture.host("claude"), "copy", None);
        from_cwd(&fixture, root)
            .args(args)
            .assert()
            .code(2)
            .stderr(contains(error));
        if fixture.root.join("config/my-skills").exists() {
            wrote_cache.push(args);
        }
    }
    assert!(
        wrote_cache.is_empty(),
        "created the config directory: {wrote_cache:?}"
    );
}
