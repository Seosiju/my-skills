mod support_h;

use std::fs;
use std::path::Path;

use assert_cmd::Command;
use predicates::prelude::*;
use support_h::{Fixture, external, link_dir, plain, skill};

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
fn imported_skill_works_with_enable_and_skills_surfaces() {
    let fixture = Fixture::repo();
    let source = external(&fixture, "# Import\n");
    import(&fixture, &source).assert().success();
    fixture
        .command()
        .args(["enable", "brand"])
        .assert()
        .success()
        .stdout("enabled: brand\n");
    fixture
        .command()
        .arg("skills")
        .assert()
        .success()
        .stdout(predicate::str::contains("brand"));
}

#[test]
fn imported_skill_works_with_enable_skills_and_install_surfaces() {
    let fixture = Fixture::repo();
    let source = external(&fixture, "# Import\n");
    import(&fixture, &source).assert().success();
    fixture
        .command()
        .args(["enable", "brand"])
        .assert()
        .success();
    fixture
        .command()
        .arg("skills")
        .assert()
        .success()
        .stdout(predicate::str::contains("brand"));
    fixture
        .command()
        .args(["install", "brand", "--host", "claude", "--dry-run"])
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
