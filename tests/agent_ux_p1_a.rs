mod support_h;

use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use my_skills::drive::state::State;
use my_skills::platform::paths::display_path;
use predicates::str::contains;
use serde_json::{Value, json};
use support_h::{Fixture, skill};

const BATCH: &str = r#"
[defaults]
install_mode = "copy"

[skills.alpha]
enabled = true
group = "batch"

[skills.beta]
enabled = false
group = "batch"
hosts = ["codex"]

[skills.gamma]
enabled = true
group = "other"
"#;

fn batch() -> Fixture {
    let fixture = Fixture::repo();
    fixture.write_manifest(BATCH);
    for name in ["alpha", "beta", "gamma"] {
        skill(
            &fixture.registry.join("skills"),
            name,
            "Fixture skill.",
            "Example.",
        );
    }
    fixture
}

fn command(fixture: &Fixture, root: &Path) -> Command {
    let mut command = fixture.command();
    command
        .env_remove("MY_SKILLS_ROOT")
        .args(["--root", root.to_str().unwrap()]);
    command
}

fn names(data: &Value, key: &str, field: &str) -> Vec<String> {
    data[key]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row[field].as_str().unwrap().to_owned())
        .collect()
}

fn other_registry(fixture: &Fixture, directory: &str, name: &str) -> PathBuf {
    let root = fixture.root.join(directory);
    skill(&root.join("skills"), name, "Fixture skill.", "Example.");
    fs::write(
        root.join("my-skills.toml"),
        format!(
            "schema_version = 1\n[defaults]\ninstall_mode = 'copy'\n\n\
             [targets.claude]\nenabled = true\npath = {}\n\
             [targets.codex]\nenabled = false\n[targets.agy]\nenabled = false\n\
             [skills.{name}]\nenabled = true\n",
            serde_json::to_string(&fixture.host("claude")).unwrap(),
        ),
    )
    .unwrap();
    root
}

#[test]
fn root_flag_wins_over_environment_cwd_and_cache_and_reports_its_source() {
    let fixture = batch();
    let active = other_registry(&fixture, "active-registry", "active");
    let selected = other_registry(&fixture, "selected-registry", "selected");
    fixture
        .command()
        .args(["set-root", active.to_str().unwrap()])
        .assert()
        .success();

    let catalog = support_h::json(
        command(&fixture, &selected)
            .env("MY_SKILLS_ROOT", &active)
            .args(["skills", "--json"]),
    );
    assert_eq!(
        catalog["registry"],
        json!({"path": display_path(&selected), "source": "flag"})
    );
    assert_eq!(names(&catalog, "skills", "name"), ["selected"]);

    let doctor = support_h::json(command(&fixture, &selected).args([
        "doctor",
        "--no-update-check",
        "--json",
    ]));
    assert_eq!(doctor["registry"]["source"], "flag");
    assert_eq!(doctor["registry"]["path"], display_path(&selected));
    assert!(doctor["registry"]["warning"].is_null());
    command(&fixture, &selected)
        .args(["doctor", "--no-update-check"])
        .assert()
        .success()
        .stdout(contains(format!(
            "Registry: {} (source: flag)",
            display_path(&selected)
        )));
}

#[test]
fn explicit_install_leaves_existing_cache_bytes_unchanged_and_never_creates_it() {
    for with_cache in [true, false] {
        let fixture = batch();
        let active = other_registry(&fixture, "active-registry", "active");
        let selected = other_registry(&fixture, "selected-registry", "selected");
        let cache = fixture.root.join("config/my-skills/root");
        let before = with_cache.then(|| {
            fs::create_dir(cache.parent().unwrap()).unwrap();
            let contents = format!("{}\r\n", active.display()).into_bytes();
            fs::write(&cache, &contents).unwrap();
            contents
        });

        let plan = support_h::json(command(&fixture, &selected).args([
            "install",
            "--yes",
            "--dry-run",
            "--json",
        ]));
        assert_eq!(plan["registry"]["source"], "flag");
        assert_eq!(plan["applicable"], true);
        fixture
            .command()
            .env_remove("MY_SKILLS_ROOT")
            .args(["install", "--yes", "--root", selected.to_str().unwrap()])
            .assert()
            .success()
            .stderr("");
        let host = fixture.host("claude");
        assert!(host.join("selected/SKILL.md").is_file());
        assert!(!host.join("alpha").exists());
        if let Some(before) = before {
            assert_eq!(fs::read(&cache).unwrap(), before);
        } else {
            assert!(!cache.parent().unwrap().exists());
        }
    }
}

#[test]
fn invalid_explicit_root_exits_two_even_when_other_roots_are_available() {
    let fixture = batch();
    let invalid = fixture.root.join("elsewhere");
    command(&fixture, &invalid)
        .env("MY_SKILLS_ROOT", &fixture.registry)
        .args(["install", "--yes"])
        .assert()
        .code(2)
        .stderr(contains(format!(
            "--root {} does not contain my-skills.toml",
            display_path(&invalid)
        )));
    assert!(!fixture.host("claude").exists());
    assert!(!fixture.root.join("config/my-skills").exists());
}

#[test]
fn explicit_root_preserves_foreign_copies_during_sync_and_blocks_batch_uninstall() {
    let fixture = batch();
    let owner = other_registry(&fixture, "owner-registry", "alpha");
    command(&fixture, &owner)
        .args(["install", "--yes"])
        .assert()
        .success();
    let installed = fixture.host("claude").join("alpha/SKILL.md");
    let before_copy = fs::read(&installed).unwrap();
    let before_state = fs::read(fixture.state()).unwrap();
    for present in [false, true] {
        let extra = if present {
            "[skills.alpha]\nenabled = false\ngroup = 'batch'\n"
        } else {
            ""
        };
        fixture.write_manifest(extra);
        command(&fixture, &fixture.registry)
            .args(["sync", "--yes"])
            .assert()
            .success();
        assert_eq!(fs::read(&installed).unwrap(), before_copy);
        assert_eq!(fs::read(fixture.state()).unwrap(), before_state);
    }

    fixture.write_manifest(BATCH);
    command(&fixture, &fixture.registry)
        .args(["uninstall", "--group", "batch", "--yes"])
        .assert()
        .code(1)
        .stdout(contains("connection belongs to another registry"));
    assert_eq!(fs::read(&installed).unwrap(), before_copy);
    assert_eq!(fs::read(fixture.state()).unwrap(), before_state);
}

#[test]
fn install_selects_the_union_including_disabled_members() {
    let fixture = batch();
    let selected = support_h::json(fixture.command().args([
        "install",
        "beta",
        "alpha",
        "alpha",
        "--group",
        "batch",
        "--host",
        "codex",
        "--dry-run",
        "--json",
    ]));
    assert_eq!(names(&selected, "actions", "skill"), ["alpha", "beta"]);
    let default = support_h::json(fixture.command().args([
        "install",
        "--host",
        "codex",
        "--dry-run",
        "--json",
    ]));
    assert_eq!(names(&default, "actions", "skill"), ["alpha", "gamma"]);
    let all = support_h::json(fixture.command().args([
        "install",
        "--all",
        "--host",
        "codex",
        "--dry-run",
        "--json",
    ]));
    assert_eq!(names(&all, "actions", "skill"), ["alpha", "beta", "gamma"]);
}

#[test]
fn builtin_seed_install_confirmation_exemption_applies_only_to_default_selection() {
    let fixture = batch();
    fs::write(
        fixture.manifest(),
        "schema_version = 1\n[defaults]\ninstall_mode = 'copy'\n\
         [skills.alpha]\nenabled = true\nsource_type = 'builtin-seed'\ngroup = 'batch'\n",
    )
    .unwrap();

    for selection in [&["alpha"][..], &["--group", "batch"]] {
        fixture
            .command()
            .arg("install")
            .args(selection)
            .assert()
            .code(2)
            .stderr(contains("writes to multiple hosts"));
    }
    fixture.command().arg("install").assert().success();
    let state = State::load(Some(&fixture.state())).unwrap();
    assert!(state.get("alpha", "claude").is_some());
    assert!(state.get("alpha", "codex").is_some());
}

#[test]
fn selected_sync_prunes_only_selected_skills_and_keeps_other_connections_and_orphans() {
    let fixture = batch();
    skill(
        &fixture.registry.join("skills"),
        "orphan",
        "Fixture skill.",
        "Example.",
    );
    fixture.write_manifest(&format!("{BATCH}\n[skills.orphan]\nenabled = true\n"));
    fixture
        .command()
        .args(["install", "--all", "--yes"])
        .assert()
        .success();
    let before = State::load(Some(&fixture.state())).unwrap();
    let manifest = BATCH.replace("enabled = true", "enabled = false");
    fixture.write_manifest(&manifest);

    let preview = support_h::json(fixture.command().args([
        "sync",
        "beta",
        "alpha",
        "--group",
        "batch",
        "--dry-run",
        "--json",
        "--yes",
    ]));
    assert_eq!(
        names(&preview, "actions", "skill"),
        ["alpha", "alpha", "beta"]
    );
    assert_eq!(names(&preview, "actions", "action"), ["REMOVE"; 3]);
    fixture
        .command()
        .args(["sync", "alpha", "beta", "--yes"])
        .assert()
        .success();
    let after = State::load(Some(&fixture.state())).unwrap();
    for host in ["claude", "codex"] {
        for name in ["alpha", "beta"] {
            assert!(after.get(name, host).is_none());
            assert!(!fixture.host(host).join(name).exists());
        }
        for name in ["gamma", "orphan"] {
            assert_eq!(after.get(name, host), before.get(name, host));
            assert!(fixture.host(host).join(name).join("SKILL.md").is_file());
        }
    }
    fixture
        .command()
        .args(["uninstall", "orphan", "--yes"])
        .assert()
        .success();
    assert!(
        State::load(Some(&fixture.state()))
            .unwrap()
            .get("orphan", "codex")
            .is_none()
    );
}

#[test]
fn batch_uninstall_plans_each_selected_skill_on_each_host_and_requires_yes() {
    let fixture = batch();
    fixture
        .command()
        .args(["install", "--all", "--yes"])
        .assert()
        .success();
    let before = fs::read(fixture.state()).unwrap();
    fixture
        .command()
        .args(["uninstall", "alpha", "--group", "batch", "--dry-run"])
        .assert()
        .success()
        .stdout(contains("alpha -> claude"))
        .stdout(contains("alpha -> codex"))
        .stdout(contains("beta -> claude"))
        .stdout(contains("beta -> codex"));
    fixture
        .command()
        .args(["uninstall", "alpha", "beta"])
        .assert()
        .code(2)
        .stderr(contains("writes to multiple hosts"));
    assert_eq!(fs::read(fixture.state()).unwrap(), before);
    fixture
        .command()
        .args(["uninstall", "alpha", "--group", "batch", "--yes"])
        .assert()
        .success();
    let state = State::load(Some(&fixture.state())).unwrap();
    for host in ["claude", "codex"] {
        assert!(state.get("alpha", host).is_none());
        assert!(state.get("beta", host).is_none());
        assert!(state.get("gamma", host).is_some());
    }
}

#[test]
fn enable_and_disable_preview_every_change_and_noop_then_edit_only_the_selection() {
    let fixture = batch();
    let before = fs::read(fixture.manifest()).unwrap();
    for (cmd, change, noop, other_state) in [
        (
            "enable",
            "would mark beta enabled",
            "alpha already enabled",
            true,
        ),
        (
            "disable",
            "would mark alpha disabled",
            "beta already disabled",
            false,
        ),
    ] {
        fixture.write_manifest(BATCH);
        fixture
            .command()
            .args([cmd, "alpha", "--group", "batch", "--dry-run"])
            .assert()
            .success()
            .stdout(contains(change))
            .stdout(contains(noop));
        assert_eq!(fs::read(fixture.manifest()).unwrap(), before);
        fixture
            .command()
            .args([cmd, "alpha", "beta", "--group", "batch"])
            .assert()
            .success();
        let config = fixture.config();
        assert_eq!(
            config["skills"]["alpha"]["enabled"].as_bool(),
            Some(other_state)
        );
        assert_eq!(
            config["skills"]["beta"]["enabled"].as_bool(),
            Some(other_state)
        );
        assert_eq!(config["skills"]["gamma"]["enabled"].as_bool(), Some(true));
    }
}

#[test]
fn invalid_batch_arguments_never_write_the_manifest_or_root_cache() {
    let fixture = batch();
    let before = fs::read(fixture.manifest()).unwrap();
    for cmd in ["enable", "disable", "install", "sync", "uninstall", "audit"] {
        for invalid in [["alpha", "unregistered"], ["--group", "unregistered"]] {
            fixture
                .command()
                .env_remove("MY_SKILLS_ROOT")
                .args([cmd, "alpha"])
                .args(invalid)
                .assert()
                .code(2)
                .stderr(contains("unknown"))
                .stderr(contains("unregistered"));
            assert_eq!(fs::read(fixture.manifest()).unwrap(), before);
            assert!(!fixture.root.join("config/my-skills").exists());
            assert!(!fixture.host("claude").exists());
        }
    }

    fs::write(
        fixture.registry.join("my-skills.local.toml"),
        "[skills.local-only]\nenabled = false\n",
    )
    .unwrap();
    fixture
        .command()
        .env_remove("MY_SKILLS_ROOT")
        .args(["disable", "gamma", "local-only"])
        .assert()
        .code(2)
        .stderr(contains("unknown skill: local-only"));
    assert_eq!(fs::read(fixture.manifest()).unwrap(), before);
    assert!(!fixture.root.join("config/my-skills").exists());
}

#[test]
fn audit_keeps_single_result_and_reports_cross_skill_bundle_findings() {
    let fixture = batch();
    let audit_fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/audit");
    for (name, example) in [("alpha", "credential-reader"), ("beta", "network-sender")] {
        let source = audit_fixtures.join(example).join("SKILL.md");
        let text = fs::read_to_string(source)
            .unwrap()
            .replace(&format!("name: {example}"), &format!("name: {name}"));
        fs::write(fixture.canonical(name).join("SKILL.md"), text).unwrap();
    }
    let single = support_h::json(fixture.command().args(["audit", "alpha", "--json"]));
    assert_eq!(single["skill"], "alpha");
    assert!(single.get("skills").is_none());
    let same = support_h::json(
        fixture
            .command()
            .args(["audit", "gamma", "--group", "other", "--json"]),
    );
    assert_eq!(same["skill"], "gamma");
    assert!(same.get("bundle_findings").is_none());

    let output = fixture
        .command()
        .args(["audit", "alpha", "--group", "batch", "--json"])
        .assert()
        .code(1)
        .stderr("");
    let bundle: Value = serde_json::from_slice(&output.get_output().stdout).unwrap();
    assert_eq!(bundle["blocked"], true);
    assert_eq!(names(&bundle, "skills", "skill"), ["alpha", "beta"]);
    assert_eq!(
        names(&bundle, "bundle_findings", "rule_id"),
        ["cross-skill-credential-network"]
    );
    let default = support_h::json(fixture.command().args(["audit", "--json"]));
    assert_eq!(names(&default, "skills", "skill"), ["alpha", "gamma"]);
    assert_eq!(default["bundle_findings"], json!([]));
}

#[test]
fn skills_json_filters_groups_with_host_and_state_and_keeps_existing_row_data() {
    let fixture = batch();
    for (filters, expected) in [
        (
            &["--group", "batch", "--host", "claude"][..],
            &["alpha"][..],
        ),
        (
            &["--group", "batch", "--host", "codex", "--disabled"],
            &["beta"],
        ),
        (
            &["--group", "batch", "--group", "other", "--enabled"],
            &["alpha", "gamma"],
        ),
    ] {
        let data = support_h::json(fixture.command().args(["skills", "--json"]).args(filters));
        assert_eq!(names(&data, "skills", "name"), expected);
    }
    let data = support_h::json(fixture.command().args(["skills", "--json"]));
    let alpha = &data["skills"][0];
    assert_eq!(
        alpha["canonical_path"],
        display_path(&fixture.canonical("alpha"))
    );
    assert_eq!(
        alpha["path"],
        display_path(&Path::new("skills").join("alpha"))
    );
    assert_eq!(alpha["group"], "batch");
    assert_eq!(alpha["description"], "Fixture skill.");
    fixture
        .command()
        .args([
            "skills",
            "--group",
            "unknown",
            "--host",
            "codex",
            "--disabled",
        ])
        .assert()
        .code(2)
        .stderr(contains("unknown group: unknown"));
}

#[test]
fn skills_brief_uses_only_the_documented_keys_and_preserves_governance_and_text() {
    let fixture = batch();
    let full = support_h::json(
        fixture
            .command()
            .args(["skills", "--json", "--with-governance"]),
    );
    let alias = support_h::json(
        fixture
            .command()
            .args(["skills", "--json", "--with-status"]),
    );
    assert_eq!(full, alias);
    let brief = support_h::json(fixture.command().args(["skills", "--json", "--brief"]));
    assert_eq!(brief["registry"], full["registry"]);
    let brief_governance = support_h::json(fixture.command().args([
        "skills",
        "--json",
        "--brief",
        "--with-governance",
    ]));
    for ((row, complete), governed) in brief["skills"]
        .as_array()
        .unwrap()
        .iter()
        .zip(full["skills"].as_array().unwrap())
        .zip(brief_governance["skills"].as_array().unwrap())
    {
        let mut expected = json!({
            "name": complete["name"], "enabled": complete["enabled"], "hosts": complete["hosts"],
            "group": complete["group"], "status": complete["status"],
        });
        assert_eq!(*row, expected);
        expected["governance"] =
            json!({"audit": complete["audit"], "provenance": complete["provenance"]});
        assert_eq!(*governed, expected);
    }
    let text = fixture
        .command()
        .args(["skills", "--with-governance"])
        .assert()
        .success();
    fixture
        .command()
        .args(["skills", "--with-governance", "--brief"])
        .assert()
        .success()
        .stdout(text.get_output().stdout.clone());
}

#[test]
fn ten_skill_catalog_brief_size() {
    let fixture = Fixture::repo();
    let mut manifest = String::new();
    for index in 0..10 {
        let name = format!("sample-{index}");
        writeln!(manifest, "\n[skills.{name}]\nenabled = true").unwrap();
        skill(
            &fixture.registry.join("skills"),
            &name,
            "Synthetic fixture for measuring catalog output.",
            "Example.",
        );
    }
    fixture.write_manifest(&manifest);
    let full = fixture
        .command()
        .args(["skills", "--json"])
        .assert()
        .success();
    let brief = fixture
        .command()
        .args(["skills", "--json", "--brief"])
        .assert()
        .success();
    let full = &full.get_output().stdout;
    let brief = &brief.get_output().stdout;
    for output in [full, brief] {
        let data: Value = serde_json::from_slice(output).unwrap();
        assert_eq!(data["skills"].as_array().unwrap().len(), 10);
        assert_eq!(data["skills"][0].get("group"), Some(&Value::Null));
    }
    println!(
        "10 skills: full {} bytes, brief {} bytes",
        full.len(),
        brief.len()
    );
    assert!(brief.len() < full.len());
}
