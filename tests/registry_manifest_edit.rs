mod support_h;

use std::fs;

use my_skills::registry::edit::{register_skill, set_skill_enabled};
use support_h::{Fixture, native};

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
