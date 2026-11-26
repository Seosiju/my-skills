#[allow(dead_code)]
mod support_g;

use std::fs::{self, OpenOptions};
use std::path::Path;
use std::time::{Duration, SystemTime};

use assert_cmd::prelude::*;
use my_skills::platform::paths;
use serde_json::{Value, json};
use support_g::{Fixture, MANIFEST, skill};

fn parse(assertion: &assert_cmd::assert::Assert) -> Value {
    serde_json::from_slice(&assertion.get_output().stdout).unwrap()
}

fn output(assertion: &assert_cmd::assert::Assert) -> &str {
    std::str::from_utf8(&assertion.get_output().stdout).unwrap()
}

fn record_json(
    skill_name: &str,
    host: &str,
    mode: &str,
    source: &str,
    destination: &Path,
    fallback_reason: Option<&str>,
) -> Value {
    let mut obj = json!({
        "skill": skill_name,
        "host": host,
        "mode": mode,
        "source": source,
        "destination": paths::display_path(destination),
        "source_hash": "sha256:00",
        "installed_hash": "sha256:00",
        "installed_at": "2026-01-01T00:00:00Z",
    });
    if let Some(reason) = fallback_reason {
        obj["fallback_reason"] = json!(reason);
    }
    obj
}

fn write_state(fixture: &Fixture, installs: Vec<Value>) -> std::path::PathBuf {
    let state_path = fixture.base.join("xdg-state/my-skills/state.json");
    fs::create_dir_all(state_path.parent().unwrap()).unwrap();
    fs::write(
        &state_path,
        json!({"schema_version": 1, "installs": installs}).to_string(),
    )
    .unwrap();
    state_path
}

fn set_past_mtime(path: &Path) -> SystemTime {
    let past = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    let file = OpenOptions::new().write(true).open(path).unwrap();
    file.set_modified(past).unwrap();
    fs::metadata(path).unwrap().modified().unwrap()
}

fn sum_host_connections(report: &Value) -> (u64, u64, u64) {
    report["hosts"]
        .as_array()
        .unwrap()
        .iter()
        .fold((0, 0, 0), |(l, c, f), host| {
            (
                l + host["connections"]["link"].as_u64().unwrap(),
                c + host["connections"]["copy"].as_u64().unwrap(),
                f + host["connections"]["fallback"].as_u64().unwrap(),
            )
        })
}

#[test]
fn t1_cwd_registry_separates_own_and_other_registry_records() {
    let fixture = Fixture::isolated();
    let reg_a = fixture.registry.clone();
    fixture.cache(&reg_a);

    let reg_b = fixture.base.join("registry-b");
    fs::create_dir_all(reg_b.join("skills/beta-one")).unwrap();
    fs::create_dir_all(reg_b.join("skills/alpha-one")).unwrap();
    fs::write(
        reg_b.join("my-skills.toml"),
        concat!(
            "schema_version = 1\nskills_root = \"skills\"\n",
            "[targets.agy]\nenabled = true\n",
            "[skills.beta-one]\nenabled = true\nhosts = [\"claude\"]\n",
            "[skills.alpha-one]\nenabled = true\nhosts = [\"claude\"]\n",
        ),
    )
    .unwrap();
    fs::write(reg_b.join("skills/beta-one/SKILL.md"), skill("beta-one")).unwrap();
    fs::write(reg_b.join("skills/alpha-one/SKILL.md"), skill("alpha-one")).unwrap();

    let dst = fixture.base.join("home/.claude/skills");
    let state_path = write_state(
        &fixture,
        vec![
            record_json(
                "alpha-one",
                "claude",
                "link",
                &paths::display_path(&reg_a.join("skills/alpha-one")),
                &dst.join("alpha-one"),
                None,
            ),
            record_json(
                "alpha-two",
                "codex",
                "link",
                &paths::display_path(&reg_a.join("skills/alpha-two")),
                &fixture.base.join("home/.agents/skills/alpha-two"),
                None,
            ),
            record_json(
                "alpha-three",
                "codex",
                "copy",
                &paths::display_path(&reg_a.join("skills/alpha-three")),
                &fixture.base.join("home/.agents/skills/alpha-three"),
                Some("link unavailable"),
            ),
            record_json(
                "beta-one",
                "claude",
                "link",
                &paths::display_path(&reg_b.join("skills/beta-one")),
                &dst.join("beta-one"),
                None,
            ),
        ],
    );

    let cache_path = fixture.base.join("xdg-config/my-skills/root");
    let state_bytes_before = fs::read(&state_path).unwrap();
    let cache_bytes_before = fs::read(&cache_path).unwrap();
    let state_mtime_before = set_past_mtime(&state_path);
    let cache_mtime_before = set_past_mtime(&cache_path);

    let text_res = fixture
        .command()
        .env_remove("MY_SKILLS_ROOT")
        .current_dir(&reg_b)
        .args(["doctor", "--no-update-check"])
        .assert()
        .success();
    let text = output(&text_res);
    let lines: Vec<&str> = text.lines().collect();
    let conn_idx = lines
        .iter()
        .position(|line| *line == "Connections: link 1 / copy 0 (fallback 0)")
        .unwrap_or_else(|| panic!("missing Connections line:\n{text}"));
    let foreign_idx = lines
        .iter()
        .position(|line| *line == "  FOREIGN: 1")
        .unwrap_or_else(|| panic!("missing FOREIGN status line:\n{text}"));
    let missing_idx = lines
        .iter()
        .position(|line| *line == "  MISSING: 1")
        .unwrap_or_else(|| panic!("missing MISSING status line:\n{text}"));
    let other_idx = lines
        .iter()
        .position(|line| {
            *line == "  Other registries or stale records: link 2 / copy 1 (fallback 1)"
        })
        .unwrap_or_else(|| panic!("missing Other registries line:\n{text}"));
    assert!(
        conn_idx < foreign_idx && foreign_idx < missing_idx && missing_idx < other_idx,
        "unexpected line order: conn={conn_idx}, foreign={foreign_idx}, missing={missing_idx}, other={other_idx}\n{text}"
    );
    #[cfg(windows)]
    {
        let warning_idx = lines
            .iter()
            .position(|line| line.starts_with("  warning: Antigravity CLI (agy) on Windows"))
            .unwrap_or_else(|| panic!("missing agy connection warning:\n{text}"));
        assert!(
            other_idx < warning_idx,
            "expected warning after Other line: other={other_idx}, warning={warning_idx}\n{text}"
        );
    }

    let json_res = fixture
        .command()
        .env_remove("MY_SKILLS_ROOT")
        .current_dir(&reg_b)
        .args(["doctor", "--no-update-check", "--json"])
        .assert()
        .success();
    let report = parse(&json_res);
    let conn = &report["connections"];
    assert_eq!(conn["scope"], "registry");
    assert_eq!(conn["link"], 1);
    assert_eq!(conn["copy"], 0);
    assert_eq!(conn["fallback"], 0);
    assert_eq!(conn["other"], json!({"link": 2, "copy": 1, "fallback": 1}));
    assert_eq!(conn["statuses"], json!({"FOREIGN": 1, "MISSING": 1}));
    assert_eq!(sum_host_connections(&report), (1, 0, 0));
    assert_eq!(
        conn["link"].as_u64().unwrap() + conn["other"]["link"].as_u64().unwrap(),
        3
    );
    assert_eq!(
        conn["copy"].as_u64().unwrap() + conn["other"]["copy"].as_u64().unwrap(),
        1
    );
    assert_eq!(
        conn["fallback"].as_u64().unwrap() + conn["other"]["fallback"].as_u64().unwrap(),
        1
    );

    // U6: doctor does not touch state.json or root cache.
    assert_eq!(fs::read(&state_path).unwrap(), state_bytes_before);
    assert_eq!(fs::read(&cache_path).unwrap(), cache_bytes_before);
    assert_eq!(
        fs::metadata(&state_path).unwrap().modified().unwrap(),
        state_mtime_before
    );
    assert_eq!(
        fs::metadata(&cache_path).unwrap().modified().unwrap(),
        cache_mtime_before
    );
}

#[test]
fn t2_active_registry_with_only_own_records_matches_exact_output() {
    let fixture = Fixture::isolated();
    fixture.write(
        "my-skills.toml",
        concat!(
            "schema_version = 1\nskills_root = \"skills\"\n",
            "[skills.alpha-one]\nenabled = true\nhosts = [\"claude\"]\n",
            "[skills.alpha-two]\nenabled = true\nhosts = [\"codex\"]\n",
        ),
    );
    fixture.write("skills/alpha-one/SKILL.md", &skill("alpha-one"));
    fixture.write("skills/alpha-two/SKILL.md", &skill("alpha-two"));
    fixture.cache(&fixture.registry);

    write_state(
        &fixture,
        vec![
            record_json(
                "alpha-one",
                "claude",
                "link",
                &paths::display_path(&fixture.registry.join("skills/alpha-one")),
                &fixture.base.join("home/.claude/skills/alpha-one"),
                None,
            ),
            record_json(
                "alpha-two",
                "codex",
                "copy",
                &paths::display_path(&fixture.registry.join("skills/alpha-two")),
                &fixture.base.join("home/.agents/skills/alpha-two"),
                Some("fallback reason"),
            ),
        ],
    );

    let text_res = fixture
        .command()
        .env_remove("MY_SKILLS_ROOT")
        .env("PATH", &fixture.bin)
        .args(["doctor", "--no-update-check"])
        .assert()
        .success();
    let text = output(&text_res);
    assert!(
        !text.contains("Other registries or stale records"),
        "unexpected Other line:\n{text}"
    );

    let hosts_start = text.find("Hosts:\n").expect("Hosts section");
    let manifest_start = text.find("\nManifest:").expect("Manifest section");
    let hosts_block = &text[hosts_start..manifest_start];
    let claude_path = paths::display_path(&fixture.base.join("home/.claude/skills"));
    let codex_path = paths::display_path(&fixture.base.join("home/.agents/skills"));
    let agy_path = paths::display_path(&fixture.base.join("home/.gemini/config/skills"));
    let expected_hosts = format!(
        concat!(
            "Hosts:\n",
            "  {:12} exe={:18} enabled=True  writable=True  path={}\n",
            "    mode=link / connections: link 1 / copy 0 (fallback 0)\n",
            "  {:12} exe={:18} enabled=True  writable=True  path={}\n",
            "    mode=link / connections: link 0 / copy 1 (fallback 1)\n",
            "  {:12} exe={:18} enabled=False writable=True  path={}\n",
            "    mode=link / connections: link 0 / copy 0 (fallback 0)\n",
        ),
        "Claude Code",
        "not found",
        claude_path,
        "Codex",
        "not found",
        codex_path,
        "Antigravity CLI",
        "not found",
        agy_path,
    );
    assert_eq!(hosts_block, expected_hosts);

    let conn_start = text.find("Connections:").expect("Connections section");
    let val_start = text
        .find("\n\nFull validation:")
        .expect("Full validation section");
    let conn_block = &text[conn_start..val_start];
    assert_eq!(
        conn_block,
        "Connections: link 1 / copy 1 (fallback 1)\n  MISSING: 2"
    );

    let json_res = fixture
        .command()
        .env_remove("MY_SKILLS_ROOT")
        .args(["doctor", "--no-update-check", "--json"])
        .assert()
        .success();
    let report = parse(&json_res);
    assert_eq!(
        report["connections"],
        json!({
            "scope": "registry",
            "link": 1,
            "copy": 1,
            "fallback": 1,
            "other": {"link": 0, "copy": 0, "fallback": 0},
            "statuses": {"MISSING": 2},
            "warnings": [],
        })
    );
}

#[test]
fn t3_deleted_old_skills_root_and_removed_host_go_to_other_while_missing_skill_stays() {
    let fixture = Fixture::isolated();
    fixture.write(
        "my-skills.toml",
        concat!(
            "schema_version = 1\nskills_root = \"custom-skills\"\n",
            "[skills.vanished-skill]\nenabled = true\nhosts = [\"codex\"]\n",
        ),
    );
    fs::create_dir_all(fixture.registry.join("custom-skills")).unwrap();

    write_state(
        &fixture,
        vec![
            // 1. Deleted registry path -> other (link on claude)
            record_json(
                "deleted-one",
                "claude",
                "link",
                &paths::display_path(&fixture.base.join("deleted-registry/skills/deleted-one")),
                &fixture.base.join("home/.claude/skills/deleted-one"),
                None,
            ),
            // 2. Old skills folder in same registry -> other (fallback copy on codex)
            record_json(
                "old-dir-one",
                "codex",
                "copy",
                &paths::display_path(&fixture.registry.join("skills/old-dir-one")),
                &fixture.base.join("home/.agents/skills/old-dir-one"),
                Some("old fallback"),
            ),
            // 3. Removed host (hermes) in current custom-skills folder -> other (link)
            record_json(
                "legacy-host-one",
                "hermes",
                "link",
                &paths::display_path(&fixture.registry.join("custom-skills/legacy-host-one")),
                &fixture.base.join("home/.hermes/skills/legacy-host-one"),
                None,
            ),
            // 4. Skill folder disappeared from current custom-skills folder -> stays in this registry (non-fallback copy on codex)
            record_json(
                "vanished-skill",
                "codex",
                "copy",
                &paths::display_path(&fixture.registry.join("custom-skills/vanished-skill")),
                &fixture.base.join("home/.agents/skills/vanished-skill"),
                None,
            ),
        ],
    );

    let json_res = fixture
        .command()
        .args(["doctor", "--no-update-check", "--json"])
        .assert()
        .success();
    let report = parse(&json_res);
    assert_eq!(
        report["connections"],
        json!({
            "scope": "registry",
            "link": 0,
            "copy": 1,
            "fallback": 0,
            "other": {"link": 2, "copy": 1, "fallback": 1},
            "statuses": {"MISSING": 1},
            "warnings": [],
        })
    );
    let host_counts = |name: &str| {
        report["hosts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|h| h["name"] == name)
            .unwrap()["connections"]
            .clone()
    };
    assert_eq!(
        host_counts("claude"),
        json!({"link": 0, "copy": 0, "fallback": 0})
    );
    assert_eq!(
        host_counts("codex"),
        json!({"link": 0, "copy": 1, "fallback": 0})
    );
    assert_eq!(
        host_counts("agy"),
        json!({"link": 0, "copy": 0, "fallback": 0})
    );
    assert_eq!(sum_host_connections(&report), (0, 1, 0));

    let text_res = fixture
        .command()
        .args(["doctor", "--no-update-check"])
        .assert()
        .success();
    let text = output(&text_res);
    assert!(text.contains("Connections: link 0 / copy 1 (fallback 0)"));
    assert!(text.contains("  Other registries or stale records: link 2 / copy 1 (fallback 1)"));
}

#[test]
fn t4_machine_scope_when_unconfigured_or_manifest_invalid() {
    let fixture = Fixture::isolated();
    write_state(
        &fixture,
        vec![
            record_json(
                "alpha-one",
                "claude",
                "link",
                &paths::display_path(&fixture.registry.join("skills/alpha-one")),
                &fixture.base.join("home/.claude/skills/alpha-one"),
                None,
            ),
            record_json(
                "alpha-two",
                "codex",
                "link",
                &paths::display_path(&fixture.base.join("other/skills/alpha-two")),
                &fixture.base.join("home/.agents/skills/alpha-two"),
                None,
            ),
            record_json(
                "alpha-three",
                "codex",
                "copy",
                &paths::display_path(&fixture.base.join("other/skills/alpha-three")),
                &fixture.base.join("home/.agents/skills/alpha-three"),
                Some("fallback"),
            ),
        ],
    );

    // 1. Unconfigured environment -> exit 0, machine scope.
    let unconf_text = fixture
        .command()
        .env_remove("MY_SKILLS_ROOT")
        .current_dir(fixture.base.join("outside"))
        .args(["doctor", "--no-update-check"])
        .assert()
        .success();
    let text = output(&unconf_text);
    assert!(
        text.contains(
            "Connections: link 2 / copy 1 (fallback 1) across all registries (no registry loaded)"
        ),
        "{text}"
    );
    assert!(
        !text.contains("Other registries or stale records"),
        "{text}"
    );

    let unconf_json = fixture
        .command()
        .env_remove("MY_SKILLS_ROOT")
        .current_dir(fixture.base.join("outside"))
        .args(["doctor", "--no-update-check", "--json"])
        .assert()
        .success();
    let report = parse(&unconf_json);
    assert_eq!(report["connections"]["scope"], "machine");
    assert_eq!(report["connections"]["link"], 2);
    assert_eq!(report["connections"]["copy"], 1);
    assert_eq!(report["connections"]["fallback"], 1);
    assert_eq!(report["connections"].get("other"), Some(&Value::Null));

    // 2. Invalid TOML manifest in cwd -> exit 1, machine scope.
    fixture.write("my-skills.toml", "invalid = [\n");
    let invalid_text = fixture
        .command()
        .env_remove("MY_SKILLS_ROOT")
        .current_dir(&fixture.registry)
        .args(["doctor", "--no-update-check"])
        .assert()
        .code(1);
    let text = output(&invalid_text);
    assert!(
        text.contains(
            "Connections: link 2 / copy 1 (fallback 1) across all registries (no registry loaded)"
        ),
        "{text}"
    );
    assert!(
        !text.contains("Other registries or stale records"),
        "{text}"
    );

    let invalid_json = fixture
        .command()
        .env_remove("MY_SKILLS_ROOT")
        .current_dir(&fixture.registry)
        .args(["doctor", "--no-update-check", "--json"])
        .assert()
        .code(1);
    let report = parse(&invalid_json);
    assert_eq!(report["connections"]["scope"], "machine");
    assert_eq!(report["connections"]["link"], 2);
    assert_eq!(report["connections"]["copy"], 1);
    assert_eq!(report["connections"]["fallback"], 1);
    assert_eq!(report["connections"].get("other"), Some(&Value::Null));
}

#[test]
fn t5_nested_registries_and_explicit_root_flag() {
    let fixture = Fixture::isolated();
    let outer = fixture.base.join("outer");
    let inner = outer.join("nested/inner");
    fs::create_dir_all(outer.join("skills")).unwrap();
    fs::create_dir_all(inner.join("skills")).unwrap();
    fs::write(outer.join("my-skills.toml"), MANIFEST).unwrap();
    fs::write(inner.join("my-skills.toml"), MANIFEST).unwrap();

    write_state(
        &fixture,
        vec![
            record_json(
                "outer-a",
                "claude",
                "link",
                &paths::display_path(&outer.join("skills/outer-a")),
                &fixture.base.join("home/.claude/skills/outer-a"),
                None,
            ),
            record_json(
                "outer-b",
                "codex",
                "link",
                &paths::display_path(&outer.join("skills/outer-b")),
                &fixture.base.join("home/.agents/skills/outer-b"),
                None,
            ),
            record_json(
                "inner-a",
                "claude",
                "link",
                &paths::display_path(&inner.join("skills/inner-a")),
                &fixture.base.join("home/.claude/skills/inner-a"),
                None,
            ),
            record_json(
                "inner-b",
                "codex",
                "copy",
                &paths::display_path(&inner.join("skills/inner-b")),
                &fixture.base.join("home/.agents/skills/inner-b"),
                Some("inner fallback"),
            ),
        ],
    );

    let outer_display = paths::display_path(&outer.canonicalize().unwrap());
    let inner_display = paths::display_path(&inner.canonicalize().unwrap());

    // Direction 1: active = outer, cwd = inner.
    fixture.cache(&outer);
    let r1 = parse(
        &fixture
            .command()
            .env_remove("MY_SKILLS_ROOT")
            .current_dir(&inner)
            .args(["doctor", "--no-update-check", "--json"])
            .assert()
            .success(),
    );
    assert_eq!(r1["registry"]["path"], inner_display);
    assert_eq!(r1["connections"]["scope"], "registry");
    assert_eq!(r1["connections"]["link"], 1);
    assert_eq!(r1["connections"]["copy"], 1);
    assert_eq!(r1["connections"]["fallback"], 1);
    assert_eq!(
        r1["connections"]["other"],
        json!({"link": 2, "copy": 0, "fallback": 0})
    );

    // Direction 2: active = inner, cwd = outer.
    fixture.cache(&inner);
    let r2 = parse(
        &fixture
            .command()
            .env_remove("MY_SKILLS_ROOT")
            .current_dir(&outer)
            .args(["doctor", "--no-update-check", "--json"])
            .assert()
            .success(),
    );
    assert_eq!(r2["registry"]["path"], outer_display);
    assert_eq!(r2["connections"]["scope"], "registry");
    assert_eq!(r2["connections"]["link"], 2);
    assert_eq!(r2["connections"]["copy"], 0);
    assert_eq!(r2["connections"]["fallback"], 0);
    assert_eq!(
        r2["connections"]["other"],
        json!({"link": 1, "copy": 1, "fallback": 1})
    );

    // --root selects inner while active is outer and cwd is outside.
    fixture.cache(&outer);
    let r3 = parse(
        &fixture
            .command()
            .env_remove("MY_SKILLS_ROOT")
            .current_dir(fixture.base.join("outside"))
            .args([
                "--root",
                inner.to_str().unwrap(),
                "doctor",
                "--no-update-check",
                "--json",
            ])
            .assert()
            .success(),
    );
    assert_eq!(r3["registry"]["path"], inner_display);
    assert_eq!(r3["registry"]["source"], "flag");
    assert_eq!(r3["connections"]["link"], 1);
    assert_eq!(r3["connections"]["copy"], 1);
    assert_eq!(r3["connections"]["fallback"], 1);
    assert_eq!(
        r3["connections"]["other"],
        json!({"link": 2, "copy": 0, "fallback": 0})
    );

    // MY_SKILLS_ROOT selects inner while active is outer and cwd is outside.
    let r4 = parse(
        &fixture
            .command()
            .env("MY_SKILLS_ROOT", &inner)
            .current_dir(fixture.base.join("outside"))
            .args(["doctor", "--no-update-check", "--json"])
            .assert()
            .success(),
    );
    assert_eq!(r4["registry"]["path"], inner_display);
    assert_eq!(r4["registry"]["source"], "env");
    assert_eq!(r4["connections"]["link"], 1);
    assert_eq!(r4["connections"]["copy"], 1);
    assert_eq!(r4["connections"]["fallback"], 1);
    assert_eq!(
        r4["connections"]["other"],
        json!({"link": 2, "copy": 0, "fallback": 0})
    );
}

#[test]
fn t6_corrupt_and_missing_state_and_no_persistent_writes() {
    // Part A: corrupt state.json -> warning, other: null, files untouched.
    let fixture = Fixture::isolated();
    fixture.cache(&fixture.registry);
    let state_path = fixture.base.join("xdg-state/my-skills/state.json");
    fs::create_dir_all(state_path.parent().unwrap()).unwrap();
    fs::write(&state_path, "{broken").unwrap();

    let cache_path = fixture.base.join("xdg-config/my-skills/root");
    let state_bytes_before = fs::read(&state_path).unwrap();
    let cache_bytes_before = fs::read(&cache_path).unwrap();
    let state_mtime_before = set_past_mtime(&state_path);
    let cache_mtime_before = set_past_mtime(&cache_path);

    let text_res = fixture
        .command()
        .env_remove("MY_SKILLS_ROOT")
        .args(["doctor", "--no-update-check"])
        .assert()
        .success();
    let text = output(&text_res);
    assert!(text.contains("Connections: link 0 / copy 0 (fallback 0)"));
    assert!(!text.contains("Other registries or stale records"));
    assert!(text.contains("invalid JSON"));

    let json_res = fixture
        .command()
        .env_remove("MY_SKILLS_ROOT")
        .args(["doctor", "--no-update-check", "--json"])
        .assert()
        .success();
    let report = parse(&json_res);
    assert_eq!(report["connections"]["scope"], "registry");
    assert_eq!(report["connections"]["link"], 0);
    assert_eq!(report["connections"]["copy"], 0);
    assert_eq!(report["connections"]["fallback"], 0);
    assert_eq!(report["connections"].get("other"), Some(&Value::Null));
    assert!(
        report["connections"]["warnings"][0]
            .as_str()
            .unwrap()
            .contains("invalid JSON")
    );

    assert_eq!(fs::read(&state_path).unwrap(), state_bytes_before);
    assert_eq!(fs::read(&cache_path).unwrap(), cache_bytes_before);
    assert_eq!(
        fs::metadata(&state_path).unwrap().modified().unwrap(),
        state_mtime_before
    );
    assert_eq!(
        fs::metadata(&cache_path).unwrap().modified().unwrap(),
        cache_mtime_before
    );

    // Part B: missing state.json and no root cache (cwd discovery) -> empty state, no files created.
    let clean = Fixture::isolated();
    let clean_state = clean.base.join("xdg-state/my-skills/state.json");
    let clean_cache_dir = clean.base.join("xdg-config");
    assert!(!clean_state.exists());
    assert!(!clean_cache_dir.exists());

    let res = clean
        .command()
        .env_remove("MY_SKILLS_ROOT")
        .current_dir(&clean.registry)
        .args(["doctor", "--no-update-check", "--json"])
        .assert()
        .success();
    let report = parse(&res);
    assert_eq!(report["registry"]["source"], "cwd");
    assert_eq!(
        report["connections"],
        json!({
            "scope": "registry",
            "link": 0,
            "copy": 0,
            "fallback": 0,
            "other": {"link": 0, "copy": 0, "fallback": 0},
            "statuses": {},
            "warnings": [],
        })
    );
    assert!(!clean_state.exists());
    assert!(!clean_cache_dir.exists());
}

#[cfg(windows)]
#[test]
fn t8_windows_verbatim_prefix_and_case_difference_count_as_owned() {
    let fixture = Fixture::isolated();
    fs::create_dir_all(fixture.registry.join("skills/win-verbatim")).unwrap();
    fs::create_dir_all(fixture.registry.join("skills/win-case")).unwrap();

    let verbatim = format!(
        r"\\?\{}",
        paths::display_path(&fixture.registry.join("skills/win-verbatim"))
    );
    let upper = paths::display_path(&fixture.registry.join("skills/win-case")).to_uppercase();

    write_state(
        &fixture,
        vec![
            record_json(
                "win-verbatim",
                "claude",
                "link",
                &verbatim,
                &fixture.base.join("home/.claude/skills/win-verbatim"),
                None,
            ),
            record_json(
                "win-case",
                "codex",
                "copy",
                &upper,
                &fixture.base.join("home/.agents/skills/win-case"),
                None,
            ),
        ],
    );

    let report = parse(
        &fixture
            .command()
            .args(["doctor", "--no-update-check", "--json"])
            .assert()
            .success(),
    );
    assert_eq!(report["connections"]["scope"], "registry");
    assert_eq!(report["connections"]["link"], 1);
    assert_eq!(report["connections"]["copy"], 1);
    assert_eq!(report["connections"]["fallback"], 0);
    assert_eq!(
        report["connections"]["other"],
        json!({"link": 0, "copy": 0, "fallback": 0})
    );
    assert_eq!(sum_host_connections(&report), (1, 1, 0));
}
