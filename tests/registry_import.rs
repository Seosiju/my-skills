//! `import <dir>` and `import --from <host>`.

mod common;

mod registry_import {
    use std::fs;
    use std::path::Path;
    use std::thread;
    use std::time::Duration;

    use crate::common::{Fixture, external, json, link_dir, plain, skill};
    use assert_cmd::Command;
    use fs4::FileExt;
    use predicates::prelude::*;
    use serde_json::{Value, json};

    fn import(fixture: &Fixture, source: &Path) -> Command {
        let mut cmd = fixture.command();
        cmd.arg("import").arg(source);
        cmd
    }

    fn assert_registration(fixture: &Fixture, enabled: bool, hosts: &[&str]) {
        let config = fixture.config();
        let skill = config["skills"]["brand"].as_table().unwrap();
        assert_eq!(skill["enabled"].as_bool(), Some(enabled));
        assert_eq!(
            skill["hosts"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect::<Vec<_>>(),
            hosts
        );
        assert_eq!(skill.len(), 2);
    }

    #[test]
    fn import_registers_new_skill_disabled_with_enabled_targets() {
        let fixture = Fixture::repo();
        let source = external(&fixture, "# Import\n");
        import(&fixture, &source).assert().success().stderr("").stdout(format!(
        "imported: brand -> {}\nregistered: brand (disabled)\nNext: `my-skills enable brand`, then `my-skills sync brand --host <host>`.\n",
        plain(&fixture.canonical("brand"))
    ));
        assert_registration(&fixture, false, &["claude", "codex"]);
    }

    #[test]
    fn import_enable_registers_new_skill_enabled() {
        let fixture = Fixture::repo();
        let source = external(&fixture, "# Import\n");
        import(&fixture, &source)
            .arg("--enable")
            .assert()
            .success()
            .stdout(predicate::str::contains(
                "Next: `my-skills sync brand --host <host>`.",
            ));
        assert_registration(&fixture, true, &["claude", "codex"]);
    }

    #[test]
    fn imported_skill_works_with_enable_skills_and_sync_surfaces() {
        let fixture = Fixture::repo();
        let source = external(&fixture, "# Import\n");
        import(&fixture, &source).assert().success();
        fixture
            .command()
            .args(["enable", "brand"])
            .assert()
            .success()
            .stdout(
                "enabled: brand
",
            );
        fixture
            .command()
            .arg("skills")
            .assert()
            .success()
            .stdout(predicate::str::contains("brand"));
        fixture
            .command()
            .args(["sync", "brand", "--host", "claude", "--dry-run"])
            .assert()
            .success()
            .stdout(predicate::str::contains("CREATE"));
    }

    #[test]
    fn import_registers_when_canonical_directory_is_identical_but_unregistered() {
        let fixture = Fixture::repo();
        let source = identical(&fixture);
        import(&fixture, &source).assert().success().stdout(
            "up to date: brand already matches the canonical skill\nregistered: brand (disabled)\n",
        );
        assert_registration(&fixture, false, &["claude", "codex"]);
    }

    fn identical(fixture: &Fixture) -> std::path::PathBuf {
        let source = external(fixture, "# Import\n");
        fs::create_dir(fixture.canonical("brand")).unwrap();
        fs::copy(
            source.join("SKILL.md"),
            fixture.canonical("brand").join("SKILL.md"),
        )
        .unwrap();
        source
    }

    fn existing_registration(fixture: &Fixture, enabled: bool) {
        fixture.write_manifest(&format!(
            "\n[skills.brand]\nenabled = {enabled}\nhosts = [\"claude\"]\n"
        ));
    }

    fn overlay(fixture: &Fixture) {
        fs::write(
            fixture.registry.join("my-skills.local.toml"),
            "[skills.brand]\nenabled = true\nhosts = [\"codex\"]\n",
        )
        .unwrap();
    }

    #[test]
    fn import_preserves_existing_registered_section_without_enable() {
        let fixture = Fixture::repo();
        let source = identical(&fixture);
        existing_registration(&fixture, true);
        import(&fixture, &source).assert().success();
        assert_registration(&fixture, true, &["claude"]);
    }

    #[test]
    fn import_enable_existing_disabled_skill_preserves_hosts() {
        let fixture = Fixture::repo();
        let source = identical(&fixture);
        existing_registration(&fixture, false);
        import(&fixture, &source).arg("--enable").assert().success();
        assert_registration(&fixture, true, &["claude"]);
    }

    #[test]
    fn import_existing_registered_skill_does_not_bake_local_overlay_into_main_manifest() {
        let fixture = Fixture::repo();
        let source = identical(&fixture);
        existing_registration(&fixture, false);
        overlay(&fixture);
        let original = fs::read(fixture.manifest()).unwrap();
        import(&fixture, &source)
            .assert()
            .success()
            .stdout(predicate::str::contains("registered: brand (enabled)"));
        assert_eq!(fs::read(fixture.manifest()).unwrap(), original);
    }

    #[test]
    fn import_enable_existing_skill_updates_main_manifest_despite_local_overlay() {
        let fixture = Fixture::repo();
        let source = identical(&fixture);
        existing_registration(&fixture, false);
        overlay(&fixture);
        import(&fixture, &source).arg("--enable").assert().success();
        assert_registration(&fixture, true, &["claude"]);
    }

    #[test]
    fn import_registers_skill_present_only_in_local_overlay() {
        let fixture = Fixture::repo();
        let source = identical(&fixture);
        overlay(&fixture);
        import(&fixture, &source).assert().success();
        assert_registration(&fixture, false, &["claude", "codex"]);
    }

    #[test]
    fn import_enable_registers_skill_present_only_in_local_overlay() {
        let fixture = Fixture::repo();
        let source = identical(&fixture);
        overlay(&fixture);
        import(&fixture, &source).arg("--enable").assert().success();
        assert_registration(&fixture, true, &["claude", "codex"]);
    }

    #[test]
    fn import_missing_skill_file_does_not_modify_manifest() {
        let fixture = Fixture::repo();
        let source = external(&fixture, "# Example\n");
        fs::remove_file(source.join("SKILL.md")).unwrap();
        let original = fs::read(fixture.manifest()).unwrap();

        import(&fixture, &source)
            .assert()
            .code(2)
            .stderr(predicate::str::contains(
                "is not an Agent Skill (no SKILL.md)",
            ));
        assert_eq!(fs::read(fixture.manifest()).unwrap(), original);
        assert_eq!(
            fs::read_dir(fixture.registry.join("skills"))
                .unwrap()
                .count(),
            0
        );
    }

    #[test]
    fn import_keeps_the_folder_identifier_and_original_contents() {
        for content in [
            "No frontmatter.\n",
            "---\nname: brand\n---\n\n# No description\n",
            "---\nname: 'Brand Draft'\ndescription: A skill.\n---\n",
        ] {
            let fixture = Fixture::repo();
            let source = external(&fixture, "# Example\n");
            fs::write(source.join("SKILL.md"), content).unwrap();
            import(&fixture, &source).assert().success();
            assert_eq!(
                fs::read(fixture.canonical("brand").join("SKILL.md")).unwrap(),
                content.as_bytes()
            );
            assert_registration(&fixture, false, &["claude", "codex"]);
        }
    }

    #[test]
    fn import_rejects_source_symlink_without_copy_or_registration() {
        let fixture = Fixture::repo();
        let source = external(&fixture, "# Example\n");
        link_dir(&fixture.root.join("elsewhere"), &source.join("refs"));
        assert_blocked(&fixture, &source, "  symlink: refs");
    }

    #[test]
    fn import_rejects_symlinked_source_root_without_registration() {
        let fixture = Fixture::repo();
        let source = external(&fixture, "# Example\n");
        let alias = fixture.root.join("source-link");
        link_dir(&source, &alias);
        assert_blocked(&fixture, &alias.join(""), "  symlink: .");
    }

    fn assert_blocked(fixture: &Fixture, source: &Path, message: &str) {
        let original = fs::read(fixture.manifest()).unwrap();
        import(fixture, source)
            .assert()
            .code(1)
            .stdout(predicate::str::contains(message))
            .stderr("\nNothing was imported (fix the source first).\n");
        assert_eq!(fs::read(fixture.manifest()).unwrap(), original);
        assert_eq!(
            fs::read_dir(fixture.registry.join("skills"))
                .unwrap()
                .count(),
            0
        );
    }

    #[test]
    fn import_new_copies_in() {
        let fixture = Fixture::repo();
        let source = external(&fixture, "# Brand\n");
        fs::create_dir(source.join("assets")).unwrap();
        fs::write(source.join("assets/art.bin"), [3, 255, 5]).unwrap();
        import(&fixture, &source).assert().success();
        assert_eq!(
            fs::read(fixture.canonical("brand").join("assets/art.bin")).unwrap(),
            [3, 255, 5]
        );
        assert_eq!(
            fs::read(fixture.canonical("brand").join("SKILL.md")).unwrap(),
            fs::read(source.join("SKILL.md")).unwrap()
        );
        assert!(!fixture.host("claude").exists());
        assert!(!fixture.state().exists());
    }

    #[test]
    fn import_and_validate_accept_binary_assets() {
        let fixture = Fixture::repo();
        let source = external(&fixture, "# Brand\n\n![Pixel](assets/pixel.png)\n");
        let assets = [
            (
                "font.woff2",
                include_bytes!("fixtures/binary-assets/font.woff2").as_slice(),
            ),
            (
                "pixel.png",
                include_bytes!("fixtures/binary-assets/pixel.png").as_slice(),
            ),
        ];
        fs::create_dir(source.join("assets")).unwrap();
        for (name, bytes) in assets {
            assert!(bytes.contains(&0));
            fs::write(source.join("assets").join(name), bytes).unwrap();
        }

        import(&fixture, &source).assert().success();
        for (name, bytes) in assets {
            assert_eq!(
                fs::read(fixture.canonical("brand").join("assets").join(name)).unwrap(),
                bytes
            );
        }
        fixture
            .command()
            .args(["validate", "brand"])
            .assert()
            .success()
            .stdout("[OK] brand\n");
    }

    #[test]
    fn import_preserves_files_containing_nul_bytes() {
        let fixture = Fixture::repo();
        let source = external(&fixture, "# Brand\n");
        let bytes = b"Plain text with a NUL byte\0\n";
        for file in ["fake.png", "notes", "notes.md"] {
            fs::write(source.join(file), bytes).unwrap();
        }
        import(&fixture, &source).assert().success();
        for file in ["fake.png", "notes", "notes.md"] {
            assert_eq!(
                fs::read(fixture.canonical("brand").join(file)).unwrap(),
                bytes
            );
        }
    }

    #[test]
    fn import_identical_is_noop() {
        let fixture = Fixture::repo();
        let source = external(&fixture, "# Brand\n");
        import(&fixture, &source).assert().success();
        let original = fs::read(fixture.manifest()).unwrap();
        import(&fixture, &source)
            .assert()
            .success()
            .stdout(predicate::str::contains("up to date: brand"));
        assert_eq!(original, fs::read(fixture.manifest()).unwrap());
    }

    #[test]
    fn import_differs_blocks_without_force() {
        let fixture = Fixture::repo();
        let source = external(&fixture, "# New\n");
        let current = skill(
            &fixture.registry.join("skills"),
            "brand",
            "Existing canonical.",
            "# Original\n",
        );
        import(&fixture, &source)
            .assert()
            .code(1)
            .stdout(format!(
                "[BLOCKED] brand: a different canonical skill already exists at\n  {}\n",
                plain(&current)
            ))
            .stderr("Re-run with --force to overwrite it. Nothing was changed.\n");
        assert!(
            fs::read_to_string(current.join("SKILL.md"))
                .unwrap()
                .contains("# Original")
        );
    }

    #[test]
    fn import_force_overwrites() {
        let fixture = Fixture::repo();
        let source = external(&fixture, "# New\n");
        let current = skill(
            &fixture.registry.join("skills"),
            "brand",
            "Existing canonical.",
            "# Original\n",
        );
        import(&fixture, &source)
            .arg("--force")
            .assert()
            .success()
            .stdout(predicate::str::contains(format!(
                "WARN: force overwrite allowed for existing canonical skill: {}",
                plain(&current)
            )));
        assert!(
            fs::read_to_string(current.join("SKILL.md"))
                .unwrap()
                .contains("# New")
        );
    }

    #[test]
    fn import_no_skillmd_blocks() {
        let fixture = Fixture::repo();
        import(&fixture, &fixture.root.join("elsewhere"))
            .assert()
            .code(2)
            .stderr(predicate::str::contains(
                "is not an Agent Skill (no SKILL.md)",
            ));
    }

    #[test]
    fn import_warns_about_malformed_frontmatter_and_keeps_the_bytes() {
        let fixture = Fixture::repo();
        let source = external(&fixture, "# Brand\n");
        let text = "---\ndescription: [unclosed\n---\n";
        fs::write(source.join("SKILL.md"), text).unwrap();
        import(&fixture, &source)
            .assert()
            .success()
            .stdout(predicate::str::contains(
                "my-skills could not read the YAML frontmatter",
            ));
        assert_eq!(
            fs::read(fixture.canonical("brand").join("SKILL.md")).unwrap(),
            text.as_bytes()
        );
    }

    #[test]
    fn import_refuses_force_over_canonical_link() {
        let fixture = Fixture::repo();
        let source = external(&fixture, "# New\n");
        let outside = skill(
            &fixture.root.join("elsewhere"),
            "brand",
            "Outside canonical.",
            "# Keep\n",
        );
        link_dir(&outside, &fixture.canonical("brand"));
        import(&fixture, &source)
            .arg("--force")
            .assert()
            .code(1)
            .stdout(predicate::str::contains(
                "canonical skill is not a real directory",
            ));
        assert_eq!(
            fixture.canonical("brand").canonicalize().unwrap(),
            outside.canonicalize().unwrap()
        );
    }

    #[test]
    fn import_refuses_a_canonical_root_inside_a_host() {
        let fixture = Fixture::repo();
        let manifest = fs::read_to_string(fixture.manifest()).unwrap();
        let root = serde_json::to_string(&fixture.host("claude").join("canonical")).unwrap();
        fs::write(fixture.manifest(), manifest.replace("\"skills\"", &root)).unwrap();
        let source = external(&fixture, "# Brand\n");
        import(&fixture, &source)
            .assert()
            .code(2)
            .stderr(predicate::str::contains(
                "registry write overlaps a host directory",
            ));
        assert!(!fixture.host("claude").exists());
    }

    #[test]
    fn import_refuses_to_copy_machine_local_skill_data() {
        let fixture = Fixture::repo();
        let data = if cfg!(windows) {
            "data/my-skills/data"
        } else {
            "data/my-skills"
        };
        let source = skill(
            &fixture.root.join(data),
            "brand",
            "Local test data.",
            "# Example\n",
        );
        let original = fs::read(fixture.manifest()).unwrap();
        import(&fixture, &source)
            .assert()
            .code(1)
            .stdout(predicate::str::contains(
                "source overlaps the machine-local skill data directory",
            ));
        assert_eq!(fs::read(fixture.manifest()).unwrap(), original);
        assert!(!fixture.canonical("brand").exists());
    }

    // ------------------------------------------------ host import (--from) & JSON

    fn import_from(fixture: &Fixture) -> Command {
        let mut command = fixture.command();
        command.args(["import", "--from", "claude"]);
        command
    }

    fn host_skill(fixture: &Fixture, name: &str) -> std::path::PathBuf {
        skill(
            &fixture.host("claude"),
            name,
            "A host-only skill.",
            "# Example\n",
        )
    }

    #[test]
    fn import_from_host_plan_json_lists_host_candidates_and_risks() {
        let fixture = Fixture::repo();
        let source = host_skill(&fixture, "brand");
        let bad = fixture.host("claude").join("bad");
        fs::create_dir(&bad).unwrap();
        fs::write(bad.join("SKILL.md"), "---\ndescription: [unclosed\n---\n").unwrap();
        let original = fs::read(fixture.manifest()).unwrap();
        let plan = json(import_from(&fixture).args(["--plan", "--json"]));
        let candidates = plan["candidates"].as_array().unwrap();
        assert_eq!(plan["from"], "claude");
        assert_eq!(plan["source"], plain(&fixture.host("claude")));
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0]["name"], "bad");
        assert_eq!(candidates[0]["canonical_status"], "missing");
        assert_eq!(candidates[0]["recommended"], "share-enable");
        assert_eq!(
            candidates[0]["choices"],
            json!(["share-enable", "share-disable", "skip"])
        );
        assert_eq!(candidates[0]["risks"].as_array().unwrap().len(), 1);
        assert_eq!(candidates[0]["risks"][0]["severity"], "warning");
        assert!(
            candidates[0]["risks"][0]["message"]
                .as_str()
                .unwrap()
                .contains("frontmatter")
        );
        assert_eq!(
            candidates[1],
            json!({
                "name": "brand",
                "description": "A host-only skill.",
                "source": plain(&source),
                "canonical": plain(&fixture.canonical("brand")),
                "canonical_status": "missing",
                "content_hash": my_skills::drive::hash::hash_directory(&source).unwrap(),
                "risks": [],
                "choices": ["share-enable", "share-disable", "skip"],
                "recommended": "share-enable",
            })
        );
        assert_eq!(fs::read(fixture.manifest()).unwrap(), original);
        assert!(!fixture.state().exists());
        assert!(!fixture.canonical("brand").exists());
    }

    #[test]
    fn import_from_host_plan_table_matches_v041() {
        let fixture = Fixture::repo();
        import_from(&fixture)
            .arg("--plan")
            .assert()
            .success()
            .stdout(format!(
                "no skills found in {}\n",
                plain(&fixture.host("claude"))
            ));
        host_skill(&fixture, "brand");
        import_from(&fixture).arg("--plan").assert().success().stdout(
        "Skill  Canonical  Recommended   Risks\n-----  ---------  ------------  -----\nbrand  missing    share-enable  none \n",
    );
    }

    #[test]
    fn import_from_host_apply_imports_registers_and_adopts_source() {
        let fixture = Fixture::repo();
        let source = host_skill(&fixture, "example");
        let contents = fs::read(source.join("SKILL.md")).unwrap();
        import_from(&fixture)
        .args(["example", "--enable"])
        .assert()
        .success()
        .stderr("")
        .stdout(format!(
            "imported: example -> {} (enabled)\nadopted: example -> claude\nNext: `my-skills sync example --host <host>`.\n",
            plain(&fixture.canonical("example"))
        ));
        let settings = fixture.config();
        let row = &settings["skills"]["example"];
        assert_eq!(row["enabled"].as_bool(), Some(true));
        assert_eq!(
            row["hosts"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect::<Vec<_>>(),
            ["claude", "codex"]
        );
        assert_eq!(
            fs::read(fixture.canonical("example").join("SKILL.md")).unwrap(),
            contents
        );
        assert_eq!(fs::read(source.join("SKILL.md")).unwrap(), contents);
        assert!(!fixture.host("codex").exists());
        let state = my_skills::drive::state::State::load(Some(&fixture.state())).unwrap();
        let record = state.get("example", "claude").unwrap();
        assert_eq!(record.source_type, "host");
        assert_eq!(
            record.mode,
            my_skills::registry::manifest::InstallMode::Copy
        );
        assert_eq!(record.destination, plain(&source));
        assert_eq!(record.installed_hash, record.source_hash);
        assert_eq!(record.source_url, plain(&source));
        assert_eq!(record.source_revision, record.source_hash);

        let catalog = json(fixture.command().args(["skills", "--json"]));
        let row = &catalog["skills"][0];
        assert_eq!(
            row["status"],
            json!({"claude": "FRESH", "codex": "MISSING"})
        );
        assert_eq!(
            row["connections"],
            json!({
                "claude": {"mode": "copy", "link_kind": null, "fallback_reason": null},
                "codex": {"mode": null, "link_kind": null, "fallback_reason": null}
            })
        );
        assert!(row.get("provenance").is_none());
    }

    #[test]
    fn import_from_host_apply_imports_registers_adopts_source_and_syncs() {
        let fixture = Fixture::repo();
        host_skill(&fixture, "example");
        import_from(&fixture)
            .args(["example", "--enable"])
            .assert()
            .success();
        fixture
            .command()
            .args(["sync", "example"])
            .assert()
            .code(2)
            .stderr(predicate::str::contains("--yes"));
        assert!(!fixture.host("codex").join("example").exists());
        fixture
            .command()
            .args(["sync", "example", "--yes"])
            .assert()
            .success()
            .stdout(predicate::str::contains("unchanged: example -> claude"));
        assert!(fixture.host("codex").join("example/SKILL.md").is_file());
    }

    #[test]
    fn import_from_host_apply_blocks_different_canonical_without_force() {
        let fixture = Fixture::repo();
        host_skill(&fixture, "brand");
        let canonical = skill(
            &fixture.registry.join("skills"),
            "brand",
            "Existing canonical.",
            "# Existing\n",
        );
        let original = fs::read(canonical.join("SKILL.md")).unwrap();
        import_from(&fixture)
        .args(["brand", "--enable"])
        .assert()
        .code(1)
        .stdout(format!(
            "[BLOCKED] brand: a different canonical skill already exists at {}; re-run with --force to overwrite it\n",
            plain(&canonical)
        ));
        assert_eq!(fs::read(canonical.join("SKILL.md")).unwrap(), original);
        import_from(&fixture)
            .args(["brand", "--enable", "--force"])
            .assert()
            .success()
            .stdout(predicate::str::starts_with(
                "WARN: force overwrite allowed for existing canonical skill\n",
            ));
        assert!(
            fs::read_to_string(canonical.join("SKILL.md"))
                .unwrap()
                .contains("# Example")
        );
    }

    #[test]
    fn import_from_host_apply_disable_registers_disabled_skill() {
        let fixture = Fixture::repo();
        host_skill(&fixture, "brand");
        import_from(&fixture)
            .args(["brand", "--disable"])
            .assert()
            .success()
            .stdout(predicate::str::contains(
                "Next: `my-skills enable brand`, then `my-skills sync brand --host <host>`.",
            ));
        assert_eq!(
            fixture.config()["skills"]["brand"]["enabled"].as_bool(),
            Some(false)
        );
        assert!(fixture.canonical("brand").join("SKILL.md").is_file());
    }

    #[test]
    fn import_from_host_rejects_links_in_apply_and_plan() {
        let fixture = Fixture::repo();
        let brand = host_skill(&fixture, "brand");
        let elsewhere = skill(
            &fixture.root.join("elsewhere"),
            "linked",
            "Outside.",
            "# Outside\n",
        );
        link_dir(&elsewhere, &fixture.host("claude").join("linked"));
        link_dir(&elsewhere, &brand.join("refs"));
        let before = fs::read(fixture.manifest()).unwrap();
        for (name, link) in [("brand", "refs"), ("linked", ".")] {
            import_from(&fixture)
                .args([name, "--enable"])
                .assert()
                .code(1)
                .stdout(format!(
                    "[BLOCKED] {name}: import source contains symlink\n  symlink: {link}\n"
                ));
        }
        let plan = json(import_from(&fixture).args(["--plan", "--json"]));
        let candidates = plan["candidates"].as_array().unwrap();
        assert_eq!(candidates.len(), 2);
        for candidate in candidates {
            assert_eq!(candidate["choices"], json!(["skip"]));
            assert_eq!(candidate["risks"][0]["severity"], "error");
            assert!(
                candidate["risks"][0]["message"]
                    .as_str()
                    .unwrap()
                    .contains("symlink")
            );
        }
        assert_eq!(fs::read(fixture.manifest()).unwrap(), before);
        assert!(!fixture.state().exists());
        assert!(!fixture.canonical("brand").exists());
        assert!(!fixture.canonical("linked").exists());
    }

    #[test]
    fn import_from_host_plan_ignores_remote_images_in_skill_content() {
        let fixture = Fixture::repo();
        fixture.write_manifest("\n[audit]\nthreshold = \"low\"\n");
        skill(
            &fixture.host("claude"),
            "brand",
            "A host-only skill.",
            "# Example\n\n![Badge](https://example.invalid/badge.png)\n",
        );
        let plan = json(import_from(&fixture).args(["--plan", "--json"]));
        let candidate = &plan["candidates"][0];
        assert_eq!(candidate["recommended"], "share-enable");
        assert_eq!(candidate["risks"], json!([]));
        import_from(&fixture)
            .args(["brand", "--enable"])
            .assert()
            .success();
    }

    #[test]
    fn import_from_host_rolls_back_canonical_and_manifest_when_state_save_fails() {
        let fixture = Fixture::repo();
        let local = host_skill(&fixture, "brand");
        let canonical = skill(
            &fixture.registry.join("skills"),
            "brand",
            "Old canonical.",
            "# Old\n",
        );
        let manifest = fs::read(fixture.manifest()).unwrap();
        let before = fs::read(canonical.join("SKILL.md")).unwrap();
        let host_before = fs::read(local.join("SKILL.md")).unwrap();
        let state = fixture.state();
        let collision = state.with_file_name("state.json.tmp");
        fs::create_dir_all(state.parent().unwrap()).unwrap();
        fs::write(&collision, "occupied\n").unwrap();

        import_from(&fixture)
            .args(["brand", "--enable", "--force"])
            .assert()
            .code(2)
            .stderr(predicate::str::contains("state.json.tmp"));
        assert_eq!(fs::read(fixture.manifest()).unwrap(), manifest);
        assert_eq!(fs::read(canonical.join("SKILL.md")).unwrap(), before);
        assert_eq!(fs::read(local.join("SKILL.md")).unwrap(), host_before);
        assert_eq!(fs::read_to_string(collision).unwrap(), "occupied\n");
        assert!(!state.exists());
        assert_eq!(
            fs::read_dir(fixture.registry.join("skills"))
                .unwrap()
                .count(),
            1
        );
    }

    #[test]
    fn import_from_host_requires_skill_and_enable_or_disable_for_apply() {
        let fixture = Fixture::repo();
        import_from(&fixture)
            .assert()
            .code(2)
            .stderr("error: import --from requires a skill name unless --plan is used\n");
        import_from(&fixture)
            .arg("brand")
            .assert()
            .code(2)
            .stderr("error: import --from requires either --enable or --disable\n");
        import_from(&fixture)
            .args(["brand", "--enable", "--disable"])
            .assert()
            .code(2);
        fixture
            .command()
            .args(["import", "--from", "ghost", "--plan"])
            .assert()
            .code(2)
            .stderr("error: unknown host: ghost\n");
    }

    #[test]
    fn import_from_host_force_replaces_canonical_link_without_touching_its_target() {
        let fixture = Fixture::repo();
        host_skill(&fixture, "brand");
        let outside = skill(
            &fixture.root.join("elsewhere"),
            "brand",
            "Outside.",
            "# Keep\n",
        );
        let before = fs::read(outside.join("SKILL.md")).unwrap();
        let canonical = fixture.canonical("brand");
        link_dir(&outside, &canonical);
        import_from(&fixture)
            .args(["brand", "--enable", "--force"])
            .assert()
            .success();
        assert_eq!(
            my_skills::drive::link::entry_kind(&canonical).unwrap(),
            my_skills::drive::link::EntryKind::Dir
        );
        assert_eq!(fs::read(outside.join("SKILL.md")).unwrap(), before);
        assert!(
            fs::read_to_string(canonical.join("SKILL.md"))
                .unwrap()
                .contains("# Example")
        );
    }

    #[test]
    fn import_from_host_dry_run_previews_without_writing_or_locking() {
        let fixture = Fixture::repo();
        host_skill(&fixture, "brand");
        let before_manifest = fs::read(fixture.manifest()).unwrap();
        // Pre-create an edit-lock file to prove host --dry-run acquires no lock.
        let edit_lock = fixture.registry.join("my-skills.toml.edit-lock");
        fs::write(&edit_lock, "busy").unwrap();

        fixture
            .command()
            .env_remove("MY_SKILLS_ROOT")
            .args([
                "import",
                "--from",
                "claude",
                "brand",
                "--enable",
                "--dry-run",
            ])
            .assert()
            .success()
            .stderr("")
            .stdout(
                predicate::str::contains("Dry run — brand passed validation; nothing written")
                    .and(predicate::str::contains(format!(
                        "  would import to: {}",
                        plain(&fixture.canonical("brand"))
                    )))
                    .and(predicate::str::contains("  would register as: enabled"))
                    .and(predicate::str::contains("  would adopt from: claude"))
                    .and(predicate::str::contains("as the active registry")),
            );

        let data = json(fixture.command().env_remove("MY_SKILLS_ROOT").args([
            "import",
            "--from",
            "claude",
            "brand",
            "--enable",
            "--dry-run",
            "--json",
        ]));
        assert_eq!(
            data,
            json!({
                "registry": {"path": plain(&fixture.registry), "source": "cwd"},
                "skill": "brand",
                "canonical_path": plain(&fixture.canonical("brand")),
                "enabled": true,
                "adopted_host": "claude",
                "identical": false,
                "overwritten": false,
                "warnings": [],
                "dry_run": true,
            })
        );

        assert_eq!(fs::read(fixture.manifest()).unwrap(), before_manifest);
        assert!(!fixture.canonical("brand").exists());
        assert!(!fixture.state().exists());
        assert!(!fixture.root.join("config/my-skills").exists());
        assert_eq!(fs::read_to_string(&edit_lock).unwrap(), "busy");
    }

    #[test]
    fn import_json_covers_success_dry_run_and_blocked_exit_1_schemas() {
        let fixture = Fixture::repo();
        let source = external(&fixture, "# Brand\n");
        let canonical = plain(&fixture.canonical("brand"));

        let dry = json(import(&fixture, &source).args(["--dry-run", "--json"]));
        assert_eq!(
            dry,
            json!({
                "registry": {"path": plain(&fixture.registry), "source": "env"},
                "skill": "brand",
                "canonical_path": canonical,
                "enabled": false,
                "adopted_host": null,
                "identical": false,
                "overwritten": false,
                "warnings": [],
                "dry_run": true,
            })
        );
        assert!(!fixture.canonical("brand").exists());

        let applied = json(import(&fixture, &source).args(["--enable", "--json"]));
        assert_eq!(
            applied,
            json!({
                "registry": {"path": plain(&fixture.registry), "source": "env"},
                "skill": "brand",
                "canonical_path": canonical,
                "enabled": true,
                "adopted_host": null,
                "identical": false,
                "overwritten": false,
                "warnings": [],
                "dry_run": false,
            })
        );

        let same = json(import(&fixture, &source).arg("--json"));
        assert_eq!(same["identical"], true);
        assert_eq!(same["overwritten"], false);
        assert_eq!(same["dry_run"], false);

        // Different canonical without --force -> exit 1 blocked JSON.
        let changed = external(&fixture, "# Changed\n");
        let conflict_out = import(&fixture, &changed).arg("--json").assert().code(1);
        let conflict: Value = serde_json::from_slice(&conflict_out.get_output().stdout).unwrap();
        assert_eq!(conflict["registry"]["source"], "env");
        assert_eq!(conflict["blocked"], true);
        assert_eq!(conflict["skill"], "brand");
        assert!(
            conflict["reason"]
                .as_str()
                .unwrap()
                .contains("a different canonical skill already exists")
        );
        assert_eq!(conflict["errors"], json!([]));
        assert_eq!(conflict["symlink"], Value::Null);

        // Symlink in source -> exit 1 blocked JSON with symlink path.
        let linked_src = skill(
            &fixture.root.join("linked-ext"),
            "linked-skill",
            "Linked.",
            "# Linked\n",
        );
        link_dir(&fixture.root.join("elsewhere"), &linked_src.join("refs"));
        let symlink_out = import(&fixture, &linked_src).arg("--json").assert().code(1);
        let symlink_json: Value = serde_json::from_slice(&symlink_out.get_output().stdout).unwrap();
        assert_eq!(
            symlink_json,
            json!({
                "registry": {"path": plain(&fixture.registry), "source": "env"},
                "blocked": true,
                "skill": "linked-skill",
                "reason": "import source contains symlink",
                "errors": [],
                "symlink": "refs",
            })
        );

        // Host mode apply JSON -> exit 0 with adopted_host.
        host_skill(&fixture, "from-claude");
        let adopted = json(import_from(&fixture).args(["from-claude", "--disable", "--json"]));
        assert_eq!(
            adopted,
            json!({
                "registry": {"path": plain(&fixture.registry), "source": "env"},
                "skill": "from-claude",
                "canonical_path": plain(&fixture.canonical("from-claude")),
                "enabled": false,
                "adopted_host": "claude",
                "identical": false,
                "overwritten": false,
                "warnings": [],
                "dry_run": false,
            })
        );
    }

    #[test]
    fn import_usage_errors_emit_json_usage_code() {
        let fixture = Fixture::repo();
        for (args, expected_message) in [
            (
                &["import", "--json"][..],
                "import requires a source path unless --from is used",
            ),
            (
                &["import", "--plan", "--json"][..],
                "--plan requires --from <host>",
            ),
            (
                &["import", "--from", "claude", "brand", "--plan", "--json"][..],
                "--plan cannot be combined with a skill name",
            ),
            (
                &["import", "--from", "claude", "--json"][..],
                "import --from requires a skill name unless --plan is used",
            ),
            (
                &["import", "--from", "claude", "brand", "--json"][..],
                "import --from requires either --enable or --disable",
            ),
        ] {
            let out = fixture
                .command()
                .args(args)
                .assert()
                .code(2)
                .stderr(predicate::str::contains(expected_message));
            let payload: Value = serde_json::from_slice(&out.get_output().stdout).unwrap();
            assert_eq!(payload["error"]["code"], "usage", "{args:?}: {payload}");
            assert_eq!(
                payload["error"]["message"], expected_message,
                "{args:?}: {payload}"
            );
        }
    }

    #[test]
    fn removed_share_command_returns_guidance_exit_2() {
        let fixture = Fixture::repo();
        let needle = "'share' was removed in v1.5.0; use 'my-skills import --from <host>'";
        fixture
            .command()
            .args(["share", "--from", "claude", "--plan"])
            .assert()
            .code(2)
            .stderr(predicate::str::contains(needle));
        let out = fixture
            .command()
            .args(["share", "--from", "claude", "--plan", "--json"])
            .assert()
            .code(2)
            .stderr(predicate::str::contains(needle));
        let payload: Value = serde_json::from_slice(&out.get_output().stdout).unwrap();
        assert_eq!(payload["error"]["code"], "usage");
        assert!(
            payload["error"]["message"]
                .as_str()
                .unwrap()
                .contains(needle),
            "{payload}"
        );
    }

    #[test]
    fn import_dry_run_in_inactive_cwd_warns_and_shows_blocked_without_writing() {
        let fixture = Fixture::repo();
        let active = fixture.root.join("active-registry");
        fs::create_dir_all(active.join("skills")).unwrap();
        fs::write(active.join("my-skills.toml"), "schema_version = 1\n").unwrap();
        fixture
            .command()
            .args(["set-root", active.to_str().unwrap()])
            .assert()
            .success();

        let source = external(&fixture, "# Inactive Path\n");
        host_skill(&fixture, "host-skill");
        let before_manifest = fs::read(fixture.manifest()).unwrap();
        let active_display = plain(&active.canonicalize().unwrap());

        // Path mode dry-run: stderr note + text BLOCKED line; JSON keeps exit 0 schema.
        fixture
            .command()
            .env_remove("MY_SKILLS_ROOT")
            .args(["import", source.to_str().unwrap(), "--dry-run"])
            .assert()
            .success()
            .stderr(predicate::str::contains(format!(
                "note: using ./my-skills.toml for this command; active registry is {active_display}"
            )))
            .stdout(
                predicate::str::contains("BLOCKED: refusing to write to cwd registry").and(
                    predicate::str::contains("Dry run — brand passed validation; nothing written"),
                ),
            );
        let path_json_out = fixture
            .command()
            .env_remove("MY_SKILLS_ROOT")
            .args(["import", source.to_str().unwrap(), "--dry-run", "--json"])
            .assert()
            .success()
            .stderr(predicate::str::contains(
                "note: using ./my-skills.toml for this command; active registry is",
            ));
        let path_json: Value = serde_json::from_slice(&path_json_out.get_output().stdout).unwrap();
        assert_eq!(path_json["dry_run"], true);
        assert_eq!(path_json["skill"], "brand");
        assert_eq!(path_json["adopted_host"], Value::Null);

        // Host mode dry-run: stderr note + text BLOCKED line; JSON keeps exit 0 schema.
        fixture
            .command()
            .env_remove("MY_SKILLS_ROOT")
            .args([
                "import",
                "--from",
                "claude",
                "host-skill",
                "--enable",
                "--dry-run",
            ])
            .assert()
            .success()
            .stderr(predicate::str::contains(format!(
                "note: using ./my-skills.toml for this command; active registry is {active_display}"
            )))
            .stdout(
                predicate::str::contains("BLOCKED: refusing to write to cwd registry").and(
                    predicate::str::contains(
                        "Dry run — host-skill passed validation; nothing written",
                    ),
                ),
            );
        let host_json_out = fixture
            .command()
            .env_remove("MY_SKILLS_ROOT")
            .args([
                "import",
                "--from",
                "claude",
                "host-skill",
                "--enable",
                "--dry-run",
                "--json",
            ])
            .assert()
            .success()
            .stderr(predicate::str::contains(
                "note: using ./my-skills.toml for this command; active registry is",
            ));
        let host_json: Value = serde_json::from_slice(&host_json_out.get_output().stdout).unwrap();
        assert_eq!(host_json["dry_run"], true);
        assert_eq!(host_json["skill"], "host-skill");
        assert_eq!(host_json["adopted_host"], "claude");

        assert_eq!(fs::read(fixture.manifest()).unwrap(), before_manifest);
        assert!(!fixture.canonical("brand").exists());
        assert!(!fixture.canonical("host-skill").exists());
        assert!(!fixture.state().exists());
    }

    #[test]
    fn import_dry_run_never_touches_registry_skills_directory() {
        let fixture = Fixture::repo();
        let source = external(&fixture, "# Dry Run\n");
        host_skill(&fixture, "host-dry");
        let skills_dir = fixture.registry.join("skills");
        let before_mtime = fs::metadata(&skills_dir).unwrap().modified().unwrap();
        // Sleep briefly so any transient temp dir creation inside skills/ would
        // produce a measurably newer directory mtime.
        thread::sleep(Duration::from_millis(50));

        import(&fixture, &source)
            .arg("--dry-run")
            .assert()
            .success();
        import_from(&fixture)
            .args(["host-dry", "--enable", "--dry-run"])
            .assert()
            .success();

        assert_eq!(fs::read_dir(&skills_dir).unwrap().count(), 0);
        assert_eq!(
            fs::metadata(&skills_dir).unwrap().modified().unwrap(),
            before_mtime,
            "dry-run must stage under OS temp dir and leave registry skills/ mtime untouched"
        );
    }

    #[test]
    fn import_json_reports_missing_skillmd_and_invalid_names_in_both_modes() {
        let fixture = Fixture::repo();

        // Missing SKILL.md -> exit 2 config JSON error in both path and host modes.
        let no_skill = fixture.root.join("missing-md");
        fs::create_dir_all(&no_skill).unwrap();
        let path_missing = import(&fixture, &no_skill).arg("--json").assert().code(2);
        let path_payload: Value =
            serde_json::from_slice(&path_missing.get_output().stdout).unwrap();
        assert_eq!(path_payload["error"]["code"], "config");
        assert!(
            path_payload["error"]["message"]
                .as_str()
                .unwrap()
                .contains("is not an Agent Skill (no SKILL.md)")
        );

        let host_missing_dir = fixture.host("claude").join("missing-host");
        fs::create_dir_all(&host_missing_dir).unwrap();
        let host_missing = import_from(&fixture)
            .args(["missing-host", "--enable", "--json"])
            .assert()
            .code(2);
        let host_payload: Value =
            serde_json::from_slice(&host_missing.get_output().stdout).unwrap();
        assert_eq!(host_payload["error"]["code"], "config");
        assert!(
            host_payload["error"]["message"]
                .as_str()
                .unwrap()
                .contains("is not an Agent Skill (no SKILL.md)")
        );

        // Invalid skill name -> exit 1 blocked JSON in both path and host modes.
        let bad_path = fixture.root.join("Bad_Name");
        fs::create_dir_all(&bad_path).unwrap();
        fs::write(
            bad_path.join("SKILL.md"),
            "---\nname: Bad_Name\ndescription: Invalid name.\n---\n",
        )
        .unwrap();
        let bad_path_out = import(&fixture, &bad_path).arg("--json").assert().code(1);
        let bad_path_json: Value =
            serde_json::from_slice(&bad_path_out.get_output().stdout).unwrap();
        assert_eq!(bad_path_json["blocked"], true);
        assert_eq!(bad_path_json["skill"], "Bad_Name");
        assert!(
            bad_path_json["reason"]
                .as_str()
                .unwrap()
                .contains("invalid registry skill name")
        );
        assert_eq!(bad_path_json["errors"], json!([]));
        assert_eq!(bad_path_json["symlink"], Value::Null);

        let bad_host = fixture.host("claude").join("Bad_Name");
        fs::create_dir_all(&bad_host).unwrap();
        fs::write(
            bad_host.join("SKILL.md"),
            "---\nname: Bad_Name\ndescription: Invalid name.\n---\n",
        )
        .unwrap();
        let bad_host_out = import_from(&fixture)
            .args(["Bad_Name", "--enable", "--json"])
            .assert()
            .code(1);
        let bad_host_json: Value =
            serde_json::from_slice(&bad_host_out.get_output().stdout).unwrap();
        assert_eq!(bad_host_json["blocked"], true);
        assert_eq!(bad_host_json["skill"], "Bad_Name");
        assert!(
            bad_host_json["reason"]
                .as_str()
                .unwrap()
                .contains("invalid registry skill name")
        );
        assert_eq!(bad_host_json["errors"], json!([]));
        assert_eq!(bad_host_json["symlink"], Value::Null);
    }

    #[test]
    fn import_from_host_reports_state_busy_json_when_state_lock_is_held() {
        let fixture = Fixture::repo();
        host_skill(&fixture, "brand");
        let before_manifest = fs::read(fixture.manifest()).unwrap();

        let lock_path = fixture.state().with_file_name("state.json.lock");
        fs::create_dir_all(lock_path.parent().unwrap()).unwrap();
        let lock_file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .unwrap();
        FileExt::lock(&lock_file).unwrap();

        let out = import_from(&fixture)
            .env("MY_SKILLS_STATE_LOCK_TIMEOUT_MS", "50")
            .args(["brand", "--enable", "--json"])
            .assert()
            .code(2)
            .stderr(predicate::str::contains("state is busy:"));
        let payload: Value = serde_json::from_slice(&out.get_output().stdout).unwrap();
        assert_eq!(payload["error"]["code"], "state_busy");
        assert!(
            payload["error"]["message"]
                .as_str()
                .unwrap()
                .starts_with("state is busy: ")
        );
        assert!(payload["error"]["hint"].is_string());

        drop(lock_file);
        assert_eq!(fs::read(fixture.manifest()).unwrap(), before_manifest);
        assert!(!fixture.canonical("brand").exists());
        assert!(!fixture.registry.join("my-skills.toml.edit-lock").exists());
    }

    #[test]
    fn import_from_host_json_reports_force_overwrite_and_identical_readoption() {
        let fixture = Fixture::repo();
        host_skill(&fixture, "brand");
        skill(
            &fixture.registry.join("skills"),
            "brand",
            "Old canonical.",
            "# Old\n",
        );

        let overwritten =
            json(import_from(&fixture).args(["brand", "--enable", "--force", "--json"]));
        assert_eq!(
            overwritten,
            json!({
                "registry": {"path": plain(&fixture.registry), "source": "env"},
                "skill": "brand",
                "canonical_path": plain(&fixture.canonical("brand")),
                "enabled": true,
                "adopted_host": "claude",
                "identical": false,
                "overwritten": true,
                "warnings": [],
                "dry_run": false,
            })
        );

        let identical = json(import_from(&fixture).args(["brand", "--enable", "--json"]));
        assert_eq!(
            identical,
            json!({
                "registry": {"path": plain(&fixture.registry), "source": "env"},
                "skill": "brand",
                "canonical_path": plain(&fixture.canonical("brand")),
                "enabled": true,
                "adopted_host": "claude",
                "identical": true,
                "overwritten": false,
                "warnings": [],
                "dry_run": false,
            })
        );
    }
}
