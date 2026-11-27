//! Registry creation, `new`, manifest edits, enable/disable, set-root, and data-path.

mod common;

mod registry_init {
    use std::fs;
    use std::path::Path;

    use crate::common::assert_with_busy_retry;
    use crate::common::{Fixture, json, native, plain};
    use assert_cmd::Command;
    use predicates::prelude::*;

    fn init(fixture: &Fixture) -> Command {
        let mut cmd = fixture.command();
        cmd.arg("init-registry").arg(&fixture.registry);
        cmd
    }

    fn git(fixture: &Fixture, args: &[&str]) -> String {
        let output = fixture.process("git").args(args).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout)
            .unwrap()
            .replace("\r\n", "\n")
    }

    #[test]
    fn init_registry_registers_seed_skills_in_manifest() {
        let fixture = Fixture::new();
        init(&fixture).arg("--no-git").assert().success().stdout(format!(
        "Created private skill registry at {}\nNext: cd there, add skills/<name>/SKILL.md, then run 'my-skills sync --dry-run'.\n",
        plain(&fixture.registry)
    ));
        let readme = fs::read_to_string(fixture.registry.join("README.md")).unwrap();
        assert!(readme.contains("my-skills skills <name> --json"));
        assert!(readme.contains("my-skills sync --dry-run"));
        assert!(readme.contains("my-skills sync --yes"));
        assert!(!readme.contains("my-skills resolve"));
        assert!(!readme.contains("my-skills install"));
        let manifest = fixture.config();
        assert_eq!(manifest["defaults"]["install_mode"].as_str(), Some("link"));
        let skills = manifest["skills"].as_table().unwrap();
        assert_eq!(skills.len(), 3);
        for (name, enabled) in [
            ("cli-inventory", true),
            ("my-skills", true),
            ("my-jira", false),
        ] {
            let row = &skills[name];
            assert_eq!(row["enabled"].as_bool(), Some(enabled));
            assert_eq!(
                row["hosts"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap())
                    .collect::<Vec<_>>(),
                ["claude", "codex", "agy"]
            );
            assert_eq!(row["source_type"].as_str(), Some("builtin-seed"));
            assert_eq!(
                row["source_revision"].as_str(),
                Some(env!("CARGO_PKG_VERSION"))
            );
        }
    }

    #[test]
    fn init_registry_does_not_restore_removed_skills_or_delete_local_data() {
        let fixture = Fixture::new();
        init(&fixture).arg("--no-git").assert().success();
        let removed = fixture.canonical("cli-inventory");
        fs::remove_dir_all(&removed).unwrap();
        let original = fs::read(fixture.manifest()).unwrap();
        let memory = fixture.root.join("data/my-skills/example/memory.json");
        fs::create_dir_all(memory.parent().unwrap()).unwrap();
        fs::write(&memory, "{\"keep\": true}\n").unwrap();
        init(&fixture).arg("--no-git").assert().code(1).stderr(
        "error: registry scaffold already exists: my-skills.toml, skills, .gitignore, README.md\n",
    );
        assert!(!removed.exists());
        assert_eq!(fs::read(fixture.manifest()).unwrap(), original);
        assert_eq!(fs::read_to_string(memory).unwrap(), "{\"keep\": true}\n");
    }

    #[test]
    fn init_registry_sync_dry_run_has_seed_actions() {
        let fixture = Fixture::new();
        init(&fixture).arg("--no-git").assert().success();
        let data = json(fixture.command().args(["sync", "--dry-run", "--json"]));
        let pairs = data["actions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|action| {
                (
                    action["skill"].as_str().unwrap(),
                    action["host"].as_str().unwrap(),
                )
            })
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            pairs,
            [
                ("cli-inventory", "claude"),
                ("cli-inventory", "codex"),
                ("my-skills", "claude"),
                ("my-skills", "codex"),
            ]
            .into_iter()
            .collect()
        );
    }

    #[test]
    fn init_registry_syncs_agy_once_enabled_in_local_override() {
        let fixture = Fixture::new();
        init(&fixture).arg("--no-git").assert().success();
        assert_eq!(
            fixture.config()["targets"]["agy"]["enabled"].as_bool(),
            Some(false)
        );
        fs::write(
            fixture.registry.join("my-skills.local.toml"),
            "[targets.agy]\nenabled = true\n",
        )
        .unwrap();
        fixture
            .command()
            .args(["sync", "my-skills", "--host", "agy", "--mode", "copy"])
            .assert()
            .success();
        assert_eq!(
            fs::read(
                fixture
                    .root
                    .join("home/.gemini/config/skills/my-skills/SKILL.md")
            )
            .unwrap(),
            fs::read(fixture.canonical("my-skills").join("SKILL.md")).unwrap()
        );
    }

    #[test]
    fn seed_default_sync_requires_confirmation_for_custom_targets() {
        let fixture = Fixture::new();
        init(&fixture).arg("--no-git").assert().success();
        let local = fixture.registry.join("my-skills.local.toml");
        fs::write(local, "[targets.claude]\npath = \"custom/claude\"\n").unwrap();
        fixture
            .command()
            .arg("sync")
            .assert()
            .code(2)
            .stderr(predicate::str::contains("writes to multiple hosts"));
    }

    #[test]
    fn seed_default_sync_requires_confirmation_for_non_default_options() {
        let fixture = Fixture::new();
        init(&fixture).arg("--no-git").assert().success();
        for args in [&["sync", "--mode", "link"][..], &["sync", "--all"][..]] {
            fixture
                .command()
                .args(args)
                .assert()
                .code(2)
                .stderr(predicate::str::contains("writes to multiple hosts"));
        }
    }

    #[test]
    fn init_registry_no_defaults_has_no_skill_manifest_entries() {
        let fixture = Fixture::new();
        init(&fixture)
            .args(["--no-defaults", "--no-git"])
            .assert()
            .success();
        assert!(!fixture.config().contains_key("skills"));
        assert_eq!(
            fs::read_dir(fixture.registry.join("skills"))
                .unwrap()
                .count(),
            0
        );
    }

    #[test]
    fn init_registry_caches_root_for_later_commands() {
        let fixture = Fixture::new();
        init(&fixture).arg("--no-git").assert().success();
        let data = json(
            fixture
                .command()
                .env_remove("MY_SKILLS_ROOT")
                .current_dir(fixture.root.join("elsewhere"))
                .args(["skills", "--json"]),
        );
        let names = data["skills"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["name"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(names, ["cli-inventory", "my-jira", "my-skills"]);
    }

    #[test]
    fn init_registry_without_path_uses_default_location_when_not_tty() {
        let fixture = Fixture::new();
        let target = fixture.root.join("home/my-agent-skills");
        fixture
            .command()
            .args(["init-registry", "--no-defaults", "--no-git"])
            .assert()
            .success()
            .stdout(predicate::str::contains(format!(
                "Created private skill registry at {}",
                plain(&target)
            )));
        assert!(target.join("my-skills.toml").is_file());
    }

    #[test]
    fn init_registry_initializes_git_repo_by_default() {
        let fixture = Fixture::new();
        init(&fixture)
            .arg("--no-defaults")
            .assert()
            .success()
            .stdout(predicate::str::starts_with(
                "Created initial registry commit\n",
            ));
        assert!(fixture.registry.join(".git").is_dir());
        assert_eq!(
            git(&fixture, &["log", "-1", "--format=%s"]),
            "chore: initialize my-skills registry\n"
        );
    }

    #[test]
    fn init_registry_initial_commit_does_not_track_preexisting_local_state() {
        let fixture = Fixture::new();
        let state = fixture.registry.join(".omc/state/private.txt");
        fs::create_dir_all(state.parent().unwrap()).unwrap();
        fs::write(&state, "local\n").unwrap();
        init(&fixture).arg("--no-defaults").assert().success();
        assert_eq!(
            git(&fixture, &["ls-tree", "-r", "--name-only", "HEAD"]),
            ".gitignore\nREADME.md\nmy-skills.toml\n"
        );
        assert_eq!(fs::read_to_string(state).unwrap(), "local\n");
    }

    #[test]
    fn init_registry_seed_copy_does_not_copy_private_source_files() {
        let fixture = Fixture::new();
        init(&fixture).arg("--with-defaults").assert().success();
        let tool_repo = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut seed_files: Vec<String> = [
            "cli-inventory/SKILL.md",
            "cli-inventory/references/inventory-schema.md",
            "cli-inventory/scripts/scan_tools.py",
            "my-jira/SKILL.md",
            "my-jira/config.example.json",
        ]
        .map(String::from)
        .into();
        // The my-skills seed list comes from the tool repo's Git index at run
        // time, so its reference set can change without editing this test,
        // while untracked local notes or editor leftovers stay out of it.
        let tracked = fixture
            .process("git")
            .arg("-C")
            .arg(tool_repo)
            .args(["ls-files", "-z", "--", "skills/my-skills"])
            .output()
            .unwrap();
        assert!(
            tracked.status.success(),
            "{}",
            String::from_utf8_lossy(&tracked.stderr)
        );
        for path in String::from_utf8(tracked.stdout).unwrap().split('\0') {
            if let Some(relative) = path.strip_prefix("skills/") {
                seed_files.push(relative.to_owned());
            }
        }
        seed_files.sort();
        assert!(seed_files.contains(&"my-skills/SKILL.md".to_owned()));
        assert!(
            seed_files
                .iter()
                .any(|file| file.starts_with("my-skills/references/")),
            "{seed_files:?}"
        );
        for file in &seed_files {
            assert_eq!(
                fs::read(fixture.registry.join("skills").join(file)).unwrap(),
                fs::read(tool_repo.join("skills").join(file)).unwrap(),
                "{file}"
            );
        }
        // Every relative link in the copied seed SKILL.md resolves.
        let copied = fixture.canonical("my-skills");
        let text = fs::read_to_string(copied.join("SKILL.md")).unwrap();
        let mut links = 0;
        for event in pulldown_cmark::Parser::new(&text) {
            if let pulldown_cmark::Event::Start(pulldown_cmark::Tag::Link { dest_url, .. }) = event
            {
                let target = dest_url.split('#').next().unwrap();
                if target.is_empty() || target.contains("://") || target.starts_with("mailto:") {
                    continue;
                }
                links += 1;
                assert!(copied.join(target).is_file(), "broken seed link {dest_url}");
            }
        }
        assert!(links > 0, "the seed SKILL.md links its references");
        let expected = format!(
            ".gitignore\nREADME.md\nmy-skills.toml\n{}\n",
            seed_files
                .iter()
                .map(|file| format!("skills/{file}"))
                .collect::<Vec<_>>()
                .join("\n")
        );
        assert_eq!(
            git(&fixture, &["ls-tree", "-r", "--name-only", "HEAD"]),
            expected
        );
        assert!(!fixture.canonical("my-jira").join("config.json").exists());
    }

    #[test]
    fn init_registry_no_git_skips_git_init() {
        let fixture = Fixture::new();
        init(&fixture)
            .args(["--no-defaults", "--no-git"])
            .assert()
            .success();
        assert!(!fixture.registry.join(".git").exists());
    }

    #[test]
    fn init_registry_missing_git_is_graceful() {
        let fixture = Fixture::new();
        init(&fixture)
            .arg("--no-defaults")
            .env("PATH", fixture.root.join("bin"))
            .assert()
            .success()
            .stdout(predicate::str::contains(
                "git command not found; skipped git init",
            ));
        assert!(!fixture.registry.join(".git").exists());
    }

    #[test]
    fn init_registry_agy_is_the_only_opt_in_target() {
        for with_defaults in [true, false] {
            let fixture = Fixture::new();
            let mut command = init(&fixture);
            command.args([
                "--no-git",
                if with_defaults {
                    "--with-defaults"
                } else {
                    "--no-defaults"
                },
            ]);
            command.assert().success();
            let manifest = fixture.config();
            let targets = manifest["targets"].as_table().unwrap();
            let agy = targets["agy"].as_table().unwrap();
            assert_eq!(agy.len(), 3);
            assert_eq!(agy["enabled"].as_bool(), Some(false));
            assert_eq!(agy["scope"].as_str(), Some("user"));
            assert_eq!(agy["path"].as_str(), Some("~/.gemini/config/skills"));
            assert_eq!(targets.len(), 3);
            for host in ["claude", "codex"] {
                assert_eq!(targets[host]["enabled"].as_bool(), Some(true));
            }
            assert_eq!(manifest.contains_key("skills"), with_defaults);
        }
    }

    #[test]
    fn init_registry_commits_scaffold_when_git_repo_already_exists() {
        let fixture = Fixture::new();
        git(&fixture, &["init"]);
        let readme = fixture.registry.join("README.md");
        fs::write(&readme, "# My Example Registry\n").unwrap();
        init(&fixture)
        .arg("--no-defaults")
        .assert()
        .success()
        .stdout(predicate::str::starts_with(
            "git repository already exists; skipped git init\nCreated initial registry commit\n",
        ));
        assert!(fs::read_to_string(readme).unwrap().starts_with(&native(
            "# My Example Registry\n\nPrivate Agent Skill Registry.\n"
        )));
        assert_eq!(
            git(&fixture, &["ls-tree", "-r", "--name-only", "HEAD"]),
            ".gitignore\nREADME.md\nmy-skills.toml\n"
        );
    }

    #[test]
    fn default_seed_skills_are_defined_once_and_available_outside_tool_repo() {
        // The executable has the wheel's allowlisted files embedded. Runtime
        // discovery of a repo or a Python package is unnecessary.
        let fixture = Fixture::new();
        let bin = assert_cmd::cargo::cargo_bin!("my-skills");
        let copied = fixture
            .root
            .join("bin")
            .join(std::path::Path::new(bin).file_name().unwrap());
        fs::copy(bin, &copied).unwrap();
        assert_with_busy_retry(
            fixture
                .process(&copied)
                .arg("init-registry")
                .arg(&fixture.registry)
                .args(["--with-defaults", "--no-git"]),
        )
        .success();
        assert_eq!(
            fs::read_dir(fixture.registry.join("skills"))
                .unwrap()
                .count(),
            3
        );
        assert!(
            fixture
                .canonical("cli-inventory")
                .join("scripts/scan_tools.py")
                .is_file()
        );
    }
}

mod w16_new {
    use std::collections::BTreeSet;
    use std::fs;
    use std::path::Path;

    use crate::common::{Fixture, link_dir};
    use my_skills::check::frontmatter::{Value as YamlValue, parse_frontmatter};
    use my_skills::platform::paths::display_path;
    use predicates::str::contains;
    use serde_json::{Value, json};

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
        assert!(
            text.contains(&format!("created: {}", display_path(&dir))),
            "{text}"
        );
        assert!(
            text.contains(&format!("created: {}", display_path(&path))),
            "{text}"
        );
        assert!(
            text.contains(&format!("1. Edit {}", display_path(&path))),
            "{text}"
        );
        assert!(text.contains("2. my-skills enable draft"), "{text}");
        assert!(
            text.contains("3. my-skills sync draft --dry-run --json, then commit"),
            "{text}"
        );

        let skill = fs::read_to_string(&path)
            .unwrap()
            .replace(char::from(13), "");
        assert!(
            skill.starts_with("---\nname: \"draft\"\ndescription: \"TODO: replace"),
            "{skill}"
        );
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
            .args([
                "new",
                "valid-one",
                "--description",
                "Has: colon, \"quotes\", and # hash.",
            ])
            .assert()
            .success();
        fixture
            .command()
            .args(["new", "valid-two"])
            .assert()
            .success();
        for name in ["valid-one", "valid-two"] {
            fixture
                .command()
                .args(["validate", name])
                .assert()
                .success()
                .stderr("");
        }
        let text = fs::read_to_string(fixture.canonical("valid-one").join("SKILL.md")).unwrap();
        assert_eq!(text.lines().next(), Some("---"));
        assert_eq!(
            parse_frontmatter(&text).unwrap().0["description"],
            YamlValue::String("Has: colon, \"quotes\", and # hash.".into())
        );
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
        assert!(
            !fixture.config()["skills"]
                .as_table()
                .unwrap()
                .contains_key("draft")
        );
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
        assert!(
            fixture
                .canonical("sketch")
                .read_dir()
                .unwrap()
                .next()
                .is_none()
        );

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
        let json = new_json(&fixture, &["alpha"], 2);
        assert_eq!(json["error"]["code"], "already_exists");
    }

    #[test]
    fn invalid_names_unknown_hosts_and_bad_descriptions_are_usage_errors() {
        let fixture = fixture();
        for name in ["Bad", "bad_name", "bad--name", "bad-", &"x".repeat(65)] {
            assert_conflict(&fixture, &[name], "invalid skill name");
        }
        assert_conflict(&fixture, &["fine", "--host", "mars"], "unknown host: mars");
        let json = new_json(&fixture, &["fine", "--host", "mars"], 2);
        assert_eq!(json["error"]["code"], "unknown_host");
        let json = new_json(&fixture, &["fine", "--description", "   "], 2);
        assert_eq!(json["error"]["code"], "usage");
        let json = new_json(&fixture, &["Bad"], 2);
        assert_eq!(json["error"]["code"], "invalid_name");
        let json = new_json(&fixture, &["fine", "--group", "Bad Group"], 2);
        assert_eq!(json["error"]["code"], "invalid_name");
        assert_conflict(
            &fixture,
            &["fine", "--group", "Bad Group"],
            "invalid group name",
        );
        assert_conflict(
            &fixture,
            &["fine", "--description", "  "],
            "must not be empty",
        );
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

    const PREFIX: &str = "invalid --description: ";

    #[test]
    fn a_canonical_dir_inside_a_host_directory_is_refused_before_writing() {
        let fixture = fixture();
        // The skills root lives inside the claude host directory.
        let inside = fixture.host("claude").join("src");
        fs::create_dir_all(&inside).unwrap();
        let text = fs::read_to_string(fixture.manifest()).unwrap().replace(
            "skills_root = \"skills\"",
            &format!("skills_root = {}", serde_json::to_string(&inside).unwrap()),
        );
        fs::write(fixture.manifest(), text).unwrap();
        let manifest = fs::read(fixture.manifest()).unwrap();
        for extra in [&[][..], &["--dry-run"][..]] {
            fixture
                .command()
                .args(["new", "draft"])
                .args(extra)
                .assert()
                .code(2)
                .stderr(contains("overlaps a host directory"));
        }
        assert!(!inside.join("draft").exists());
        assert_eq!(fs::read(fixture.manifest()).unwrap(), manifest);
    }

    #[test]
    fn first_discovery_caches_the_registry_but_an_explicit_root_never_does() {
        let fixture = fixture();
        let cache = fixture.root.join("config/my-skills/root");
        fixture.command().args(["new", "one"]).assert().success();
        assert!(
            !cache.exists(),
            "MY_SKILLS_ROOT is explicit: nothing cached"
        );
        fixture
            .command()
            .env_remove("MY_SKILLS_ROOT")
            .args(["new", "two"])
            .assert()
            .success();
        let cached = fs::read_to_string(&cache).unwrap();
        assert_eq!(Path::new(cached.trim()), fixture.registry);
    }

    #[test]
    fn blank_description_errors_share_one_prefix_and_lengths_only_warn() {
        let fixture = fixture();
        let manifest = fs::read(fixture.manifest()).unwrap();
        let before = tree(&fixture.registry);
        for args in [
            &["--description", "  "][..],
            &["--host", "codex", "--description", "\t"][..],
        ] {
            let output = fixture
                .command()
                .args(["new", "draft"])
                .args(args)
                .args(["--json"])
                .assert()
                .code(2);
            let json: Value = serde_json::from_slice(&output.get_output().stdout).unwrap();
            let message = json["error"]["message"].as_str().unwrap();
            assert!(message.starts_with(PREFIX), "{message}");
            assert_eq!(message, format!("{PREFIX}must not be empty"));
        }
        assert_eq!(fs::read(fixture.manifest()).unwrap(), manifest);
        assert_eq!(tree(&fixture.registry), before);

        for (name, length) in [("fits", 800), ("edge", 1024), ("longer", 1025)] {
            let description = "가".repeat(length);
            let output = new_json(&fixture, &[name, "--description", &description], 0);
            let expected = if length > 1024 {
                json!([
                    "Codex shows a shortened description (1025 characters). It keeps the first 1021 characters and adds \"...\". If you want the full text to appear, keep it to 1024 characters or fewer."
                ])
            } else {
                Value::Null
            };
            assert_eq!(output["warnings"], expected);
            let text = fs::read_to_string(fixture.canonical(name).join("SKILL.md")).unwrap();
            assert_eq!(
                parse_frontmatter(&text).unwrap().0["description"],
                YamlValue::String(description)
            );
            fixture
                .command()
                .args(["validate", name])
                .assert()
                .success();
        }
        fixture
            .command()
            .args([
                "new",
                "preview",
                "--description",
                &"x".repeat(1100),
                "--dry-run",
            ])
            .assert()
            .success()
            .stdout(contains(
                "Codex shows a shortened description (1100 characters)",
            ));
        assert!(!fixture.canonical("preview").exists());
    }

    #[test]
    fn long_description_warnings_follow_the_explicit_or_enabled_hosts() {
        let fixture = fixture();
        let description = "가".repeat(1600);
        let claude = "Claude Code shows a shortened description (1600 characters). It keeps the first 1535 characters and adds \"…\". If you want the full text to appear, keep it to 1536 characters or fewer.";
        let codex = "Codex shows a shortened description (1600 characters). It keeps the first 1021 characters and adds \"...\". If you want the full text to appear, keep it to 1024 characters or fewer.";
        let base = ["draft", "--description", &description, "--dry-run"];
        for (hosts, expected) in [
            (&[][..], json!([claude, codex])),
            (&["--host", "claude"][..], json!([claude])),
            (&["--host", "codex"][..], json!([codex])),
            (&["--host", "agy"][..], Value::Null),
            (&["--host", "agy", "--host", "claude"][..], json!([claude])),
        ] {
            let result = new_json(&fixture, &[&base[..], hosts].concat(), 0);
            assert_eq!(result["warnings"], expected, "{hosts:?}: {result}");
        }

        let manifest = fs::read_to_string(fixture.manifest()).unwrap().replace(
            "[targets.codex]\nenabled = true",
            "[targets.codex]\nenabled = false",
        );
        fs::write(fixture.manifest(), manifest).unwrap();
        assert_eq!(new_json(&fixture, &base, 0)["warnings"], json!([claude]));
        assert_eq!(
            new_json(&fixture, &[&base[..], &["--host", "codex"]].concat(), 0)["warnings"],
            json!([codex])
        );
        assert!(!fixture.canonical("draft").exists());
    }

    #[test]
    fn new_uses_codex_whitespace_rules_without_changing_the_saved_description() {
        let fixture = fixture();
        let description = format!(
            "{} \t\u{a0}{}{}",
            "a".repeat(500),
            " ".repeat(100),
            "b".repeat(500)
        );
        let output = new_json(
            &fixture,
            &["draft", "--host", "codex", "--description", &description],
            0,
        );
        assert!(output.get("warnings").is_none(), "{output}");
        let text = fs::read_to_string(fixture.canonical("draft").join("SKILL.md")).unwrap();
        assert_eq!(
            parse_frontmatter(&text).unwrap().0["description"],
            YamlValue::String(description)
        );
        let validation =
            crate::common::json(fixture.command().args(["validate", "draft", "--json"]));
        assert_eq!(validation["skills"][0]["warnings"], json!([]));
    }

    #[test]
    fn new_quotes_scalar_like_names_and_preserves_line_separators_in_descriptions() {
        let fixture = fixture();
        let description = "One # heading\u{85}Two\u{2028}Three\u{2029}Four";
        for name in ["off", "0123"] {
            new_json(&fixture, &[name, "--description", description], 0);
            let text = fs::read_to_string(fixture.canonical(name).join("SKILL.md")).unwrap();
            let metadata = parse_frontmatter(&text).unwrap().0;
            assert_eq!(metadata["name"], YamlValue::String(name.into()));
            assert_eq!(
                metadata["description"],
                YamlValue::String(description.into())
            );
        }
    }
}

mod registry_manifest_edit {
    use std::fs;

    use crate::common::{Fixture, native};
    use my_skills::registry::edit::{register_skill, set_skill_enabled};

    #[test]
    fn disable_and_enable_update_skill_enabled() {
        let fixture = Fixture::repo();
        fixture.write_manifest(
            "\n[skills.alpha]\n  enabled=true # replaced by v0.4.1\nhosts = [\"claude\"]\n",
        );
        let path = fixture.manifest();
        let original = fs::read_to_string(&path).unwrap();
        fixture
            .command()
            .args(["disable", "alpha"])
            .assert()
            .success()
            .stdout("disabled: alpha\n")
            .stderr("");
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            native(&original.replace("  enabled=true # replaced by v0.4.1", "enabled = false",))
        );
        fixture
            .command()
            .args(["enable", "alpha"])
            .assert()
            .success()
            .stdout("enabled: alpha\n")
            .stderr("");
        assert_eq!(
            fixture.config()["skills"]["alpha"]["enabled"].as_bool(),
            Some(true)
        );
    }

    #[test]
    fn enable_unknown_skill_blocks() {
        let fixture = Fixture::repo();
        fixture
            .command()
            .args(["enable", "missing"])
            .assert()
            .code(2)
            .stderr("error: unknown skill: missing\n");
    }

    #[test]
    fn register_skill_appends_new_simple_section() {
        let fixture = Fixture::repo();
        register_skill(
            &fixture.manifest(),
            "brand",
            false,
            &["claude".into(), "codex".into()],
        )
        .unwrap();
        assert_eq!(
            fixture.config()["skills"]["brand"]["enabled"].as_bool(),
            Some(false)
        );
        assert!(
            fs::read_to_string(fixture.manifest())
                .unwrap()
                .ends_with(&native(
                    "\n[skills.brand]\nenabled = false\nhosts = [\"claude\", \"codex\"]\n",
                ))
        );
    }

    #[test]
    fn register_skill_rejects_invalid_name() {
        let fixture = Fixture::repo();
        let path = fixture.manifest();
        let before = fs::read(&path).unwrap();
        let error = register_skill(&path, "Bad_Name", true, &["claude".into()]).unwrap_err();
        assert!(
            error
                .to_string()
                .starts_with("invalid skill name 'Bad_Name'")
        );
        assert_eq!(fs::read(&path).unwrap(), before);
    }

    // Expected bytes generated with v0.4.1 manifest_edit.py on the inputs below.
    #[test]
    fn manifest_edits_match_v041_goldens() {
        let fixture = Fixture::repo();
        let path = fixture.manifest();
        let cases = [
            (
                "# registry\r\n [skills.alpha]   \r\n\tenabled\t=false # old\r\n # keep\r\n hosts = [\"claude\"]\r\n\r\n # last   \r\n \r\n",
                false,
                "# registry\n [skills.alpha]   \nenabled = true\n # keep\n hosts = [\"claude\"]\n\n # last\n",
            ),
            (
                "[skills.alpha]\n# both keys absent\n\n[skills.beta]\nenabled=true\n",
                true,
                "[skills.alpha]\n# both keys absent\n\nhosts = [\"claude\", \"codex\"]\nenabled = true\n[skills.beta]\nenabled=true\n",
            ),
            (
                "schema_version = 1\n\n# keep this\n",
                true,
                "schema_version = 1\n\n# keep this\n\n[skills.alpha]\nenabled = true\nhosts = [\"claude\", \"codex\"]\n",
            ),
            (
                "[skills.alpha]\n# insert before the next table\n\n[skills.beta]\nenabled = true",
                false,
                "[skills.alpha]\n# insert before the next table\n\nenabled = true\n[skills.beta]\nenabled = true\n",
            ),
            (
                "[skills.alpha]\nenabled = true\n  \n",
                false,
                "[skills.alpha]\nenabled = true\n",
            ),
        ];
        for (input, register, output) in cases {
            fs::write(&path, input).unwrap();
            if register {
                register_skill(&path, "alpha", true, &["claude".into(), "codex".into()]).unwrap();
            } else {
                set_skill_enabled(&path, "alpha", true).unwrap();
            }
            assert_eq!(fs::read(&path).unwrap(), native(output).as_bytes());
        }
    }

    #[test]
    fn busy_manifest_edit_preserves_existing_bytes_and_lock() {
        let fixture = Fixture::repo();
        fixture.write_manifest("\n[skills.alpha]\nenabled = false\n");
        let original = fs::read(fixture.manifest()).unwrap();
        let lock = fixture.manifest().with_extension("toml.edit-lock");
        fs::write(&lock, "another writer\n").unwrap();
        fixture
            .command()
            .args(["enable", "alpha"])
            .assert()
            .code(2)
            .stderr(predicates::str::contains("my-skills.toml.edit-lock"));
        assert_eq!(fs::read(fixture.manifest()).unwrap(), original);
        assert_eq!(fs::read_to_string(lock).unwrap(), "another writer\n");
    }
}

mod set_root_data_path {
    //! `set-root`, `data-path`, and `enable`/`disable --dry-run` on a small
    //! registry with one enabled skill.

    use std::fs;

    use assert_cmd::Command;
    use my_skills::platform::paths;
    use predicates::str::contains;

    use crate::common::{Fixture, skill};

    fn registry() -> Fixture {
        let fixture = Fixture::repo();
        skill(
            &fixture.registry.join("skills"),
            "alpha",
            "Demo skill alpha.",
            "Body.\n",
        );
        fixture.write_manifest("\n[skills.alpha]\nenabled = true\n");
        fixture
    }

    /// The CLI, run outside the registry with `MY_SKILLS_ROOT` set.
    fn cmd(fixture: &Fixture) -> Command {
        let mut cmd = fixture.command();
        cmd.current_dir(&fixture.root);
        cmd
    }

    #[test]
    fn data_path_uses_xdg_data_home_and_rejects_bad_names() {
        let fixture = registry();
        cmd(&fixture)
            .args(["data-path", "alpha", "--create"])
            .assert()
            .success()
            .stdout(contains("my-skills"));
        #[cfg(not(windows))]
        assert!(fixture.root.join("data/my-skills/alpha").is_dir());
        #[cfg(windows)]
        assert!(fixture.root.join("data/my-skills/data/alpha").is_dir());
        cmd(&fixture).args(["data-path", "../x"]).assert().code(2);
    }

    #[test]
    fn set_root_dry_run_writes_nothing_then_set_root_records_the_registry() {
        let fixture = registry();
        let root = fixture.registry.display().to_string();
        let expected_display = paths::display_path(&fixture.registry.canonicalize().unwrap());

        let result = cmd(&fixture)
            .env_remove("MY_SKILLS_ROOT")
            .args(["set-root", &root, "--dry-run"])
            .assert()
            .success();
        let stdout = String::from_utf8_lossy(&result.get_output().stdout);
        assert_eq!(
            stdout.trim(),
            format!("Dry run — would set active registry root to {expected_display}")
        );
        assert!(
            !stdout.contains(r"\\?\"),
            "set-root --dry-run output should not contain verbatim prefix: {stdout}"
        );
        assert!(!fixture.root_cache().exists());

        let result = cmd(&fixture)
            .env_remove("MY_SKILLS_ROOT")
            .args(["set-root", &root])
            .assert()
            .success();
        let stdout = String::from_utf8_lossy(&result.get_output().stdout);
        assert_eq!(
            stdout.trim(),
            format!("Active registry root set to {expected_display}")
        );
        assert!(
            !stdout.contains(r"\\?\"),
            "set-root output should not contain verbatim prefix: {stdout}"
        );
        let cached = fs::read_to_string(fixture.root_cache()).unwrap();
        assert!(cached.trim().ends_with("registry"), "{cached}");
    }

    #[test]
    fn enable_and_disable_dry_run_leave_the_manifest_alone() {
        let fixture = registry();
        let before = fs::read_to_string(fixture.manifest()).unwrap();
        cmd(&fixture)
            .args(["disable", "alpha", "--dry-run"])
            .assert()
            .success()
            .stdout(contains("would mark alpha disabled (currently enabled)"));
        cmd(&fixture)
            .args(["enable", "alpha", "--dry-run"])
            .assert()
            .success();
        assert_eq!(fs::read_to_string(fixture.manifest()).unwrap(), before);
        cmd(&fixture)
            .args(["disable", "nope", "--dry-run"])
            .assert()
            .code(2);
    }
}
