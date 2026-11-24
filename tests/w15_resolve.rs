mod support_h;

use std::fs;
use std::path::Path;

use my_skills::platform::paths::display_path;
use serde_json::{Value, json};
use support_h::{Fixture, link_dir, skill};

fn fixture() -> Fixture {
    let fixture = Fixture::repo();
    fixture.write_manifest("[skills.alpha]\n[skills.beta]\n");
    for name in ["alpha", "beta"] {
        let dir = skill(
            &fixture.registry.join("skills"),
            name,
            "A skill for path lookup tests.",
            "Instructions.",
        );
        write(&dir.join("references/x.md"), "Reference.");
    }
    fixture
}

fn write(path: &Path, content: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

fn record(skill: &str, host: &str, mode: &str, source: &Path, destination: &Path) -> Value {
    json!({
        "skill": skill, "host": host, "mode": mode, "source": source, "destination": destination,
        "source_hash": "unused", "installed_hash": "unused", "installed_at": "2026-01-01T00:00:00Z",
    })
}

fn save_records(fixture: &Fixture, records: &[Value]) {
    write(
        &fixture.state(),
        serde_json::to_vec_pretty(&json!({"schema_version": 1, "installs": records})).unwrap(),
    );
}

fn resolve(fixture: &Fixture, target: impl AsRef<Path>, code: i32) -> Value {
    let output = fixture
        .command()
        .env_remove("MY_SKILLS_ROOT")
        .args(["resolve", "--json"])
        .arg(target.as_ref())
        .assert()
        .code(code);
    serde_json::from_slice(&output.get_output().stdout).unwrap()
}

fn assert_target(value: &Value, dir: &Path, path: &Path) {
    assert_eq!(value["canonical_dir"], display_path(dir));
    assert_eq!(value["canonical_path"], display_path(path));
}

#[test]
fn resolves_names_drafts_and_canonical_aliases_and_preserves_uniform_schema() {
    let fixture = fixture();
    let alpha = fixture.canonical("alpha");
    write(
        &fixture.registry.join("AGENTS.md"),
        "# Fixture instructions",
    );
    let result = resolve(&fixture, "alpha", 0);
    assert_eq!(
        result,
        json!({
            "skill": "alpha", "state": "registered", "registered": true,
            "registry": {"path": display_path(&fixture.registry), "source": "cwd"},
            "canonical_dir": display_path(&alpha), "canonical_path": display_path(&alpha),
            "matched_host": null, "mode": null, "reason": null, "write_refusal": null,
            "registry_agents_md": display_path(&fixture.registry.join("AGENTS.md")),
        })
    );
    let text = fixture
        .command()
        .args(["resolve", "alpha"])
        .assert()
        .success();
    assert_eq!(
        String::from_utf8_lossy(&text.get_output().stdout)
            .lines()
            .collect::<Vec<_>>(),
        [display_path(&alpha)],
    );
    assert!(!fixture.root.join("config/my-skills").exists());
    assert!(!fixture.state().exists());

    let draft = fixture.canonical("draft");
    let draft_file = draft.join("notes.md");
    write(&draft_file, "Draft.");
    for path in [&draft, &draft_file] {
        let result = resolve(&fixture, path, 0);
        assert_eq!(result["state"], "unregistered");
        assert_eq!(result["registered"], false);
        assert_eq!(result["skill"], "draft");
        assert_target(&result, &draft, path);
    }
    let top_level_file = fixture.registry.join("skills/README.md");
    write(&top_level_file, "Not a skill directory.");
    for path in [
        fixture.registry.clone(),
        fixture.registry.join("skills"),
        fixture.canonical("absent"),
        top_level_file,
        fixture.root.join("elsewhere"),
    ] {
        let result = resolve(&fixture, path, 1);
        assert_eq!(result["state"], "unmanaged");
        assert!(result["reason"].is_string());
        assert!(result["canonical_path"].is_null());
    }
    let alias = fixture.root.join("alias");
    link_dir(&alpha, &alias);
    let result = resolve(&fixture, alias.join("references/x.md"), 0);
    assert_target(&result, &alpha, &alpha.join("references/x.md"));
    let alias_escape = format!("{}/../elsewhere", display_path(&alias));
    assert_eq!(resolve(&fixture, alias_escape, 1)["state"], "unsafe");
    let relative = resolve(&fixture, "./skills/alpha/references/../SKILL.md", 0);
    assert_target(&relative, &alpha, &alpha.join("SKILL.md"));

    let unknown = resolve(&fixture, "absent", 2);
    assert_eq!(unknown["error"]["code"], "unknown_skill");
    assert_eq!(unknown["error"]["message"], "unknown skill: absent");
}

#[test]
fn resolves_link_and_copy_files_and_preserves_cache_and_state_bytes() {
    let fixture = fixture();
    let alpha = fixture.canonical("alpha");
    let linked = fixture.host("claude").join("alpha");
    // A disabled host can retain a connection left over from an earlier sync.
    let copied = fixture.host("agy").join("alpha");
    fs::create_dir_all(linked.parent().unwrap()).unwrap();
    link_dir(&alpha, &linked);
    write(&copied.join("references/x.md"), "Copied reference.");
    write(&copied.join("host-only.md"), "Only on this host.");
    save_records(
        &fixture,
        &[
            record("alpha", "claude", "link", &alpha, &linked),
            record("alpha", "agy", "copy", &alpha, &copied),
        ],
    );
    let other = fixture.root.join("other-registry");
    write(&other.join("my-skills.toml"), "schema_version = 1\n");
    let cache = fixture.root.join("config/my-skills/root");
    write(&cache, format!("{}\r\n", display_path(&other)));
    let before_cache = fs::read(&cache).unwrap();
    let before_state = fs::read(fixture.state()).unwrap();

    for (dir, host, mode) in [(&linked, "claude", "link"), (&copied, "agy", "copy")] {
        let result = resolve(&fixture, dir.join("references/x.md"), 0);
        assert_eq!(result["state"], "registered");
        assert_eq!(result["registered"], true);
        assert_eq!(result["skill"], "alpha");
        assert_eq!(result["matched_host"], host);
        assert_eq!(result["mode"], mode);
        assert!(result["registry_agents_md"].is_null());
        assert!(
            result["write_refusal"]
                .as_str()
                .unwrap()
                .starts_with("refusing to write to cwd registry")
        );
        assert_target(&result, &alpha, &alpha.join("references/x.md"));
    }
    let host_only = resolve(&fixture, copied.join("host-only.md"), 1);
    assert_eq!(host_only["state"], "unmanaged");
    assert_eq!(host_only["matched_host"], "agy");
    assert_eq!(host_only["mode"], "copy");
    assert!(host_only["canonical_path"].is_null());
    assert!(
        host_only["reason"]
            .as_str()
            .unwrap()
            .contains("only in the host copy of a managed skill")
    );

    assert_eq!(fs::read(cache).unwrap(), before_cache);
    assert_eq!(fs::read(fixture.state()).unwrap(), before_state);
}

#[test]
fn reports_foreign_untracked_and_unsafe_host_paths() {
    let fixture = fixture();
    let alpha = fixture.canonical("alpha");
    let host = fixture.host("claude");
    let foreign = host.join("foreign");
    let foreign_source = fixture.root.join("elsewhere/skills/foreign");
    let wrong_link = host.join("alpha");
    let untracked_link = host.join("untracked-link");
    let untracked = host.join("untracked");
    write(&foreign.join("SKILL.md"), "Foreign copy.");
    write(&untracked.join("SKILL.md"), "Untracked skill.");
    link_dir(&fixture.canonical("beta"), &wrong_link);
    link_dir(&fixture.canonical("beta"), &untracked_link);
    save_records(
        &fixture,
        &[
            record("alpha", "claude", "link", &alpha, &wrong_link),
            record("foreign", "claude", "copy", &foreign_source, &foreign),
        ],
    );
    for (path, state, mode) in [
        (foreign.join("SKILL.md"), "foreign", json!("copy")),
        (wrong_link.join("SKILL.md"), "unsafe", json!("link")),
        (untracked_link.join("SKILL.md"), "unsafe", Value::Null),
        (untracked.join("SKILL.md"), "unmanaged", Value::Null),
    ] {
        let result = resolve(&fixture, path, 1);
        assert_eq!(result["state"], state);
        assert_eq!(result["matched_host"], "claude");
        assert_eq!(result["mode"], mode);
        assert!(result["canonical_path"].is_null());
        assert!(result["reason"].is_string());
    }

    // Preserve the literal parent component even when the fixture root has
    // a Windows verbatim prefix, whose PathBuf::push would otherwise fold it.
    let escape = format!("{}/alpha/../beta/SKILL.md", display_path(&host));
    assert_eq!(resolve(&fixture, escape, 1)["state"], "unsafe");
    let escape = resolve(&fixture, "./skills/alpha/../beta/SKILL.md", 1);
    assert_eq!(escape["state"], "unsafe");
    let nested = alpha.join("external");
    link_dir(&fixture.root.join("elsewhere"), &nested);
    assert_eq!(
        resolve(&fixture, nested.join("missing.md"), 1)["state"],
        "unsafe"
    );
    let inside = alpha.join("internal");
    link_dir(&alpha.join("references"), &inside);
    assert_eq!(
        resolve(&fixture, inside.join("x.md"), 0)["state"],
        "registered"
    );
}

#[test]
fn orphaned_host_records_resolve_to_the_existing_draft() {
    let fixture = fixture();
    fixture.write_manifest("[skills.beta]\n");
    let alpha = fixture.canonical("alpha");
    let linked = fixture.host("claude").join("alpha");
    let copied = fixture.host("agy").join("alpha");
    fs::create_dir_all(linked.parent().unwrap()).unwrap();
    link_dir(&alpha, &linked);
    write(&copied.join("references/x.md"), "Copied reference.");
    save_records(
        &fixture,
        &[
            record("alpha", "claude", "link", &alpha, &linked),
            record("alpha", "agy", "copy", &alpha, &copied),
        ],
    );

    let canonical_file = alpha.join("references/x.md");
    let canonical = resolve(&fixture, &canonical_file, 0);
    assert_eq!(canonical["state"], "unregistered");
    for (entry, host, mode) in [(linked, "claude", "link"), (copied, "agy", "copy")] {
        let result = resolve(&fixture, entry.join("references/x.md"), 0);
        assert_eq!(result["state"], canonical["state"], "{mode}");
        assert_eq!(result["registered"], false);
        assert_eq!(result["matched_host"], host);
        assert_eq!(result["mode"], mode);
        assert!(result["reason"].is_null());
        assert_target(&result, &alpha, &canonical_file);
    }
}

#[test]
fn host_records_without_a_canonical_directory_are_unmanaged() {
    let fixture = fixture();
    let alpha = fixture.canonical("alpha");
    let linked = fixture.host("claude").join("alpha");
    let copied = fixture.host("agy").join("alpha");
    fs::create_dir_all(linked.parent().unwrap()).unwrap();
    link_dir(&alpha, &linked);
    write(&copied.join("SKILL.md"), "Copied skill.");
    save_records(
        &fixture,
        &[
            record("alpha", "claude", "link", &alpha, &linked),
            record("alpha", "agy", "copy", &alpha, &copied),
        ],
    );
    fs::rename(&alpha, fixture.root.join("elsewhere/alpha")).unwrap();
    assert!(fs::symlink_metadata(&linked).is_ok());
    assert!(!linked.exists());

    for registered in [false, true] {
        fixture.write_manifest(if registered { "[skills.alpha]\n" } else { "" });
        for (entry, host, mode) in [(&linked, "claude", "link"), (&copied, "agy", "copy")] {
            for target in [entry.clone(), entry.join("SKILL.md")] {
                let result = resolve(&fixture, target, 1);
                assert_eq!(result["state"], "unmanaged");
                assert_eq!(result["registered"], registered);
                assert_eq!(result["matched_host"], host);
                assert_eq!(result["mode"], mode);
                assert!(result["canonical_path"].is_null());
                assert!(
                    result["reason"]
                        .as_str()
                        .unwrap()
                        .contains("canonical skill directory"),
                    "{result}"
                );
            }
        }
    }
}

#[test]
fn a_host_entry_with_the_wrong_kind_is_unmanaged() {
    for (mode, replacement) in [
        ("link", "directory"),
        ("link", "file"),
        ("link", "missing"),
        ("copy", "file"),
        ("copy", "link"),
        ("copy", "missing"),
    ] {
        let fixture = fixture();
        let alpha = fixture.canonical("alpha");
        let installed = fixture.host("claude").join("alpha");
        let target = match replacement {
            "directory" => {
                let file = installed.join("host-only.md");
                write(&file, "A local file.");
                file
            }
            "file" => {
                write(&installed, "A local file.");
                installed.clone()
            }
            "link" => {
                fs::create_dir_all(installed.parent().unwrap()).unwrap();
                link_dir(&alpha, &installed);
                installed.clone()
            }
            "missing" => installed.clone(),
            _ => unreachable!(),
        };
        save_records(
            &fixture,
            &[record("alpha", "claude", mode, &alpha, &installed)],
        );

        let result = resolve(&fixture, target, 1);
        assert_eq!(
            result["state"], "unmanaged",
            "{mode} replaced with {replacement}"
        );
        assert_eq!(result["matched_host"], "claude");
        assert_eq!(result["mode"], mode);
        assert!(result["canonical_path"].is_null());
        assert!(
            result["reason"]
                .as_str()
                .unwrap()
                .contains(&format!("recorded {mode} connection")),
            "{result}"
        );
    }
}

#[test]
fn linked_canonical_roots_resolve_consistently_and_still_reject_link_escapes() {
    let fixture = fixture();
    let alpha = fixture.canonical("alpha");
    let linked = fixture.host("claude").join("alpha");
    let copied = fixture.host("agy").join("alpha");
    let external = fixture.root.join("elsewhere/alpha");
    fs::rename(&alpha, &external).unwrap();
    link_dir(&external, &alpha);
    fs::create_dir_all(linked.parent().unwrap()).unwrap();
    link_dir(&alpha, &linked);
    write(&copied.join("references/x.md"), "Copied reference.");
    save_records(
        &fixture,
        &[
            record("alpha", "claude", "link", &alpha, &linked),
            record("alpha", "agy", "copy", &alpha, &copied),
        ],
    );

    let named = resolve(&fixture, "alpha", 0);
    assert_target(&named, &alpha, &alpha);
    for target in [&alpha, &linked, &copied] {
        let root = resolve(&fixture, target, 0);
        assert_target(&root, &alpha, &alpha);
        let file = resolve(&fixture, target.join("references/x.md"), 0);
        assert_target(&file, &alpha, &alpha.join("references/x.md"));
        assert_eq!(file["state"], named["state"]);
        assert_eq!(file["registered"], named["registered"]);
    }

    let unrelated = fixture.root.join("unrelated");
    write(&unrelated.join("x.md"), "Outside the skill.");
    link_dir(&unrelated, &external.join("escape"));
    for target in [&alpha, &linked, &copied] {
        let result = resolve(&fixture, target.join("escape/x.md"), 1);
        assert_eq!(result["state"], "unsafe");
        assert!(result["canonical_path"].is_null());
    }
}

#[test]
fn broken_canonical_links_do_not_resolve_by_name_or_path() {
    let fixture = fixture();
    let alpha = fixture.canonical("alpha");
    let external = fixture.root.join("elsewhere/alpha");
    fs::rename(&alpha, &external).unwrap();
    link_dir(&external, &alpha);
    fs::rename(&external, fixture.root.join("elsewhere/moved-alpha")).unwrap();
    assert!(!alpha.exists());

    for target in [Path::new("alpha"), &alpha] {
        let result = resolve(&fixture, target, 1);
        assert_eq!(result["state"], "unmanaged");
        assert!(result["canonical_path"].is_null());
    }
}

#[test]
fn shared_destinations_are_ambiguous_only_when_sources_differ() {
    let fixture = fixture();
    let host = fixture.host("claude");
    let manifest = fs::read_to_string(fixture.manifest()).unwrap().replace(
        &serde_json::to_string(&fixture.host("codex")).unwrap(),
        &serde_json::to_string(&host).unwrap(),
    );
    fs::write(fixture.manifest(), manifest).unwrap();
    let installed = host.join("alpha");
    let file = installed.join("references/x.md");
    write(&file, "Copied reference.");
    let alpha = fixture.canonical("alpha");
    let first = record("alpha", "claude", "copy", &alpha, &installed);
    save_records(
        &fixture,
        &[
            first.clone(),
            record("alpha", "codex", "copy", &alpha, &installed),
        ],
    );
    let shared = resolve(&fixture, &file, 0);
    assert_eq!(shared["state"], "registered");
    assert_target(&shared, &alpha, &alpha.join("references/x.md"));

    save_records(
        &fixture,
        &[
            first,
            record(
                "beta",
                "codex",
                "copy",
                &fixture.canonical("beta"),
                &installed,
            ),
        ],
    );
    let ambiguous = resolve(&fixture, &file, 1);
    assert_eq!(ambiguous["state"], "ambiguous");
    assert!(ambiguous["reason"].is_string());
    assert!(ambiguous["canonical_path"].is_null());
}

#[cfg(windows)]
#[test]
fn windows_matches_case_insensitively_and_expands_tilde_without_folding_escapes() {
    let fixture = fixture();
    let alpha = fixture.canonical("alpha");
    let home = fixture.root.join("home");
    let host = home.join("fixture-host");
    let entry = host.join("alpha");
    let manifest = fs::read_to_string(fixture.manifest()).unwrap().replace(
        &serde_json::to_string(&fixture.host("claude")).unwrap(),
        "'~/fixture-host'",
    );
    fs::write(fixture.manifest(), manifest).unwrap();
    write(&entry.join("references/x.md"), "Copied reference.");
    save_records(
        &fixture,
        &[record("alpha", "claude", "copy", &alpha, &entry)],
    );
    let result = resolve(&fixture, "~/FIXTURE-HOST/ALPHA/references/x.md", 0);
    assert_eq!(result["matched_host"], "claude");
    let result = resolve(&fixture, "~/fixture-host/alpha/../beta", 1);
    assert_eq!(result["state"], "unsafe");
}
