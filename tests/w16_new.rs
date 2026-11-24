mod support_h;

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use my_skills::platform::paths::display_path;
use predicates::str::contains;
use serde_json::{Value, json};
use support_h::{Fixture, link_dir};

fn fixture() -> Fixture {
    let fixture = Fixture::repo();
    fixture.write_manifest("\n[skills.alpha]\nenabled = true\n");
    fs::create_dir_all(fixture.canonical("alpha")).unwrap();
    fs::write(
        fixture.canonical("alpha").join("SKILL.md"),
        "---\nname: alpha\ndescription: An existing skill.\n---\n\nBody.\n",
    )
    .unwrap();
    fixture
}

fn tree(root: &Path) -> BTreeSet<String> {
    fn walk(dir: &Path, root: &Path, out: &mut BTreeSet<String>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            out.insert(path.strip_prefix(root).unwrap().display().to_string());
            if path.is_dir() {
                walk(&path, root, out);
            }
        }
    }
    let mut out = BTreeSet::new();
    walk(root, root, &mut out);
    out
}

fn new_json(fixture: &Fixture, args: &[&str], code: i32) -> Value {
    let output = fixture
        .command()
        .arg("new")
        .args(args)
        .arg("--json")
        .assert()
        .code(code);
    serde_json::from_slice(&output.get_output().stdout).unwrap()
}

#[test]
fn text_output_creates_a_disabled_draft_and_lists_the_next_steps() {
    let fixture = fixture();
    let dir = fixture.canonical("draft");
    let path = dir.join("SKILL.md");
    let output = fixture
        .command()
        .args(["new", "draft"])
        .assert()
        .success()
        .stderr("");
    let text = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    assert!(text.contains(&format!("created: {}", display_path(&dir))), "{text}");
    assert!(text.contains(&format!("created: {}", display_path(&path))), "{text}");
    assert!(text.contains(&format!("1. Edit {}", display_path(&path))), "{text}");
    assert!(text.contains("2. my-skills enable draft"), "{text}");
    assert!(
        text.contains("3. my-skills sync draft --dry-run --json, then commit"),
        "{text}"
    );

    let skill = fs::read_to_string(&path).unwrap().replace(char::from(13), "");
    assert!(skill.starts_with("---\nname: draft\ndescription: \"TODO: replace"), "{skill}");
    let config = fixture.config();
    let section = config["skills"]["draft"].as_table().unwrap();
    assert_eq!(section["enabled"].as_bool(), Some(false));
    assert!(!section.contains_key("group") && !section.contains_key("hosts"));
    // The existing registration and the hand-written targets are untouched.
    assert_eq!(config["skills"]["alpha"]["enabled"].as_bool(), Some(true));
    assert!(config["targets"]["claude"]["path"].is_str());
}

#[test]
fn json_output_reports_every_key_and_honors_group_hosts_and_description() {
    let fixture = fixture();
    let result = new_json(
        &fixture,
        &[
            "drafty",
            "--group",
            "lab",
            "--host",
            "claude",
            "--host",
            "codex",
            "--host",
            "claude",
            "--description",
            "Checks \"quoted\" text: and colons.",
        ],
        0,
    );
    let dir = fixture.canonical("drafty");
    let path = dir.join("SKILL.md");
    assert_eq!(
        result,
        json!({
            "registry": {"path": display_path(&fixture.registry), "source": "env"},
            "skill": "drafty", "enabled": false, "group": "lab",
            "hosts": ["claude", "codex"],
            "canonical_dir": display_path(&dir), "canonical_path": display_path(&path),
            "created": [display_path(&dir), display_path(&path)],
            "next": [
                format!("Edit {}", display_path(&path)),
                "my-skills enable drafty",
                "my-skills sync drafty --dry-run --json, then commit",
            ],
        })
    );
    let section = fixture.config()["skills"]["drafty"].clone();
    assert_eq!(section["group"].as_str(), Some("lab"));
    let hosts: Vec<_> = section["hosts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|host| host.as_str().unwrap())
        .collect();
    assert_eq!(hosts, ["claude", "codex"]);
    assert_eq!(section["enabled"].as_bool(), Some(false));

    // No group or hosts: the keys are null and empty in JSON.
    let plain = new_json(&fixture, &["plain"], 0);
    assert_eq!(plain["group"], Value::Null);
    assert_eq!(plain["hosts"], json!([]));
}

#[test]
fn the_new_skill_passes_validate_and_is_a_listed_disabled_skill() {
    let fixture = fixture();
    fixture
        .command()
        .args(["new", "valid-one", "--description", "Has: colon, \"quotes\", and # hash."])
        .assert()
        .success();
    fixture.command().args(["new", "valid-two"]).assert().success();
    for name in ["valid-one", "valid-two"] {
        fixture
            .command()
            .args(["validate", name])
            .assert()
            .success()
            .stderr("");
    }
    let output = fixture
        .command()
        .args(["skills", "--json", "--brief", "valid-one"])
        .assert()
        .success();
    let rows: Value = serde_json::from_slice(&output.get_output().stdout).unwrap();
    assert_eq!(rows["skills"][0]["enabled"], false, "{rows}");
}

#[test]
fn dry_run_writes_nothing_and_reports_dry_run_true() {
    let fixture = fixture();
    let manifest = fs::read(fixture.manifest()).unwrap();
    let before = tree(&fixture.registry);
    let result = new_json(&fixture, &["draft", "--group", "lab", "--dry-run"], 0);
    assert_eq!(result["dry_run"], true);
    assert_eq!(result["skill"], "draft");
    assert_eq!(result["group"], "lab");
    assert_eq!(result["enabled"], false);
    assert_eq!(
        result["canonical_path"],
        display_path(&fixture.canonical("draft").join("SKILL.md"))
    );
    assert_eq!(tree(&fixture.registry), before);
    assert_eq!(fs::read(fixture.manifest()).unwrap(), manifest);

    fixture
        .command()
        .args(["new", "draft", "--dry-run"])
        .assert()
        .success()
        .stdout(contains("Dry run"))
        .stdout(contains("would create:"));
    assert_eq!(tree(&fixture.registry), before);
    assert!(!fixture.config()["skills"].as_table().unwrap().contains_key("draft"));
    // A real write never reports dry_run.
    assert!(new_json(&fixture, &["draft"], 0).get("dry_run").is_none());
}

fn assert_conflict(fixture: &Fixture, args: &[&str], needle: &str) {
    let manifest = fs::read(fixture.manifest()).unwrap();
    let before = tree(&fixture.registry);
    let output = fixture
        .command()
        .arg("new")
        .args(args)
        .assert()
        .code(2)
        .stderr(contains(needle));
    assert!(output.get_output().stdout.is_empty());
    let json = new_json(fixture, args, 2);
    assert!(
        json["error"]["message"].as_str().unwrap().contains(needle),
        "{json}"
    );
    assert_eq!(fs::read(fixture.manifest()).unwrap(), manifest);
    assert_eq!(tree(&fixture.registry), before);
}

#[test]
fn an_existing_directory_file_or_broken_link_blocks_the_name() {
    let fixture = fixture();
    // An unregistered draft directory.
    fs::create_dir_all(fixture.canonical("sketch")).unwrap();
    assert_conflict(&fixture, &["sketch"], "skill already exists");
    assert!(fixture.canonical("sketch").read_dir().unwrap().next().is_none());

    // A plain file.
    fs::write(fixture.canonical("note"), "not a dir").unwrap();
    assert_conflict(&fixture, &["note"], "skill already exists");

    // A broken link: point it at a directory, then remove the directory.
    let target = fixture.root.join("elsewhere/gone");
    fs::create_dir_all(&target).unwrap();
    link_dir(&target, &fixture.canonical("dangling"));
    fs::remove_dir(&target).unwrap();
    assert!(fs::symlink_metadata(fixture.canonical("dangling")).is_ok());
    assert!(!fixture.canonical("dangling").exists());
    assert_conflict(&fixture, &["dangling"], "skill already exists");
}

#[test]
fn a_name_registered_in_either_manifest_is_a_conflict() {
    let fixture = fixture();
    // Registered in the base manifest with no directory.
    let base = fs::read_to_string(fixture.manifest()).unwrap();
    fs::write(fixture.manifest(), format!("{base}\n[skills.ghost]\n")).unwrap();
    assert_conflict(&fixture, &["ghost"], "already registered");
    // Registered only in the machine-local manifest.
    fs::write(
        fixture.registry.join("my-skills.local.toml"),
        "[skills.local-only]\nenabled = false\n",
    )
    .unwrap();
    assert_conflict(&fixture, &["local-only"], "already registered");
    // Registered and present.
    assert_conflict(&fixture, &["alpha"], "skill already exists");
}

#[test]
fn invalid_names_unknown_hosts_and_bad_descriptions_are_usage_errors() {
    let fixture = fixture();
    for name in ["Bad", "bad_name", "bad--name", "bad-"] {
        assert_conflict(&fixture, &[name], "invalid skill name");
    }
    assert_conflict(&fixture, &["fine", "--host", "mars"], "unknown host: mars");
    let json = new_json(&fixture, &["fine", "--host", "mars"], 2);
    assert_eq!(json["error"]["code"], "unknown_host");
    assert_conflict(&fixture, &["fine", "--group", "Bad Group"], "invalid group name");
    assert_conflict(&fixture, &["fine", "--description", "  "], "must not be empty");
    let long = "x".repeat(1025);
    assert_conflict(&fixture, &["fine", "--description", &long], "exceeds 1024");
    // A new group is fine.
    new_json(&fixture, &["fine", "--group", "brand-new-group"], 0);
}

#[test]
fn an_inactive_cwd_registry_is_refused_unless_the_root_is_explicit() {
    let fixture = fixture();
    let active = fixture.root.join("elsewhere/active");
    fs::create_dir_all(&active).unwrap();
    fs::write(active.join("my-skills.toml"), "schema_version = 1\n").unwrap();
    let cache = fixture.root.join("config/my-skills/root");
    fs::create_dir_all(cache.parent().unwrap()).unwrap();
    fs::write(&cache, format!("{}\n", active.display())).unwrap();

    let before = tree(&fixture.registry);
    let output = fixture
        .command()
        .env_remove("MY_SKILLS_ROOT")
        .args(["new", "draft", "--json"])
        .assert()
        .code(2)
        .stderr(contains("refusing to write to cwd registry"));
    let json: Value = serde_json::from_slice(&output.get_output().stdout).unwrap();
    assert_eq!(json["error"]["code"], "inactive_registry");
    assert_eq!(tree(&fixture.registry), before);
    // Dry runs report the same refusal instead of claiming success.
    fixture
        .command()
        .env_remove("MY_SKILLS_ROOT")
        .args(["new", "draft", "--dry-run"])
        .assert()
        .code(2)
        .stderr(contains("refusing to write to cwd registry"));

    fixture
        .command()
        .env_remove("MY_SKILLS_ROOT")
        .arg("--root")
        .arg(&fixture.registry)
        .args(["new", "draft"])
        .assert()
        .success();
    assert!(fixture.canonical("draft").join("SKILL.md").is_file());
}

#[test]
fn a_busy_manifest_edit_lock_blocks_new_without_changing_anything() {
    let fixture = fixture();
    let lock = fixture.registry.join("my-skills.toml.edit-lock");
    fs::write(&lock, "").unwrap();
    let before = tree(&fixture.registry);
    fixture.command().args(["new", "draft"]).assert().code(2);
    assert_eq!(tree(&fixture.registry), before);
    fixture
        .command()
        .args(["new", "draft", "--dry-run"])
        .assert()
        .code(2);
    // The lock belongs to someone else and stays.
    assert!(lock.exists());
}

#[cfg(unix)]
#[test]
fn a_failed_manifest_edit_removes_only_the_new_directory_and_the_lock() {
    let fixture = fixture();
    // The edit path refuses a manifest that is not a regular file.
    let real = fixture.root.join("elsewhere/real.toml");
    fs::rename(fixture.manifest(), &real).unwrap();
    std::os::unix::fs::symlink(&real, fixture.manifest()).unwrap();
    let manifest = fs::read(&real).unwrap();
    let before = tree(&fixture.registry);
    fixture
        .command()
        .args(["new", "draft"])
        .assert()
        .code(2)
        .stderr(contains("not a regular file"));
    assert!(!fixture.canonical("draft").exists());
    assert_eq!(tree(&fixture.registry), before);
    assert_eq!(fs::read(&real).unwrap(), manifest);
    assert!(fixture.canonical("alpha").join("SKILL.md").is_file());
}
