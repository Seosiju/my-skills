mod support_h;

use std::fs;

use serde_json::{Value, json};
use support_h::Fixture;

const HOSTS: [&str; 3] = ["claude", "codex", "agy"];
const MISSING_DESCRIPTION: [&str; 3] = [
    "Claude Code uses the first body line",
    "Codex does not show this skill because its description is missing",
    "Antigravity CLI shows an empty description",
];
const NAME_DIFFERS: [&str; 3] = [
    "Claude Code lists this skill as",
    "Codex lists this skill as",
    "Antigravity CLI lists this skill as",
];
const TRUNCATED: [&str; 2] = [
    "It keeps the first 1535 characters and adds \"…\".",
    "It keeps the first 1021 characters and adds \"...\".",
];
const MISSING_LINK: &str = "Referenced supporting file not found: missing.md.";
const YAML_COMMENT: &str = "A YAML comment cuts off 'description'. Agents see \"Some text\".";

struct Canary {
    name: &'static str,
    content: String,
    shared_warnings: &'static [&'static str],
    host_warnings: [&'static [&'static str]; 3],
}

impl Canary {
    fn new(name: &'static str, content: String) -> Self {
        Self {
            name,
            content,
            shared_warnings: &[],
            host_warnings: [&[]; 3],
        }
    }
}

fn canaries() -> Vec<Canary> {
    let skill = |yaml| format!("---\n{yaml}\n---\nBody first line.\n");
    let described = |name, description| skill(format!("name: {name}\ndescription: {description}"));
    vec![
        Canary::new("minimal", described("minimal", "A minimal skill.")),
        Canary::new("no-name", skill("description: A skill.".into())),
        Canary {
            host_warnings: [
                &[MISSING_DESCRIPTION[0]],
                &[MISSING_DESCRIPTION[1]],
                &[MISSING_DESCRIPTION[2]],
            ],
            ..Canary::new("no-description", skill("name: no-description".into()))
        },
        Canary {
            host_warnings: [
                &["Claude Code lists this skill as \"different-name\" using the folder name"],
                &["Codex lists this skill as \"other-name\" using the YAML name"],
                &["Antigravity CLI lists this skill as \"other-name\" using the YAML name"],
            ],
            ..Canary::new("different-name", described("other-name", "A skill."))
        },
        Canary {
            host_warnings: [
                &["Claude Code lists this skill as \"name-format\" using the folder name"],
                &["Codex lists this skill as \"Upper_case Name\" using the YAML name"],
                &["Antigravity CLI lists this skill as \"Upper_case Name\" using the YAML name"],
            ],
            ..Canary::new("name-format", described("Upper_case Name", "A skill."))
        },
        Canary {
            host_warnings: [
                &[NAME_DIFFERS[0]],
                &["Codex does not show this skill because its name is 70 characters"],
                &[NAME_DIFFERS[2]],
            ],
            ..Canary::new("long-name", described(&"x".repeat(70), "A skill."))
        },
        Canary::new("desc-800", described("desc-800", &"가".repeat(800))),
        Canary {
            host_warnings: [&[], &[TRUNCATED[1]], &[]],
            ..Canary::new("desc-1100", described("desc-1100", &"가".repeat(1100)))
        },
        Canary {
            host_warnings: [&[TRUNCATED[0]], &[TRUNCATED[1]], &[]],
            ..Canary::new("desc-2000", described("desc-2000", &"가".repeat(2000)))
        },
        Canary {
            host_warnings: [
                &["Claude Code ignores these YAML fields: custom-field."],
                &["Codex ignores these YAML fields: custom-field."],
                &["Antigravity CLI ignores these YAML fields: custom-field."],
            ],
            ..Canary::new(
                "extra-field",
                skill("name: extra-field\ndescription: A skill.\ncustom-field: Value".into()),
            )
        },
        Canary {
            shared_warnings: &[MISSING_LINK],
            ..Canary::new(
                "links",
                described("links", "A skill.")
                    + concat!(
                        "\n[Missing](missing.md)\n",
                        "````md\n```\n[Not a link](backticks.md)\n````\n",
                        "~~~md\n[Not a link](tildes.md)\n~~~\n",
                        "> ```md\n> [Not a link](quoted.md)\n> ```\n",
                        "- ~~~md\n  [Not a link](listed.md)\n  ~~~\n",
                        "`[Inline](one.md)` and `` ` [Inline](two.md) ``\n",
                        "``Multiline\n[Inline](three.md)``\n",
                    ),
            )
        },
        Canary {
            host_warnings: [
                &[],
                &["Codex does not show this skill because SKILL.md starts with a UTF-8 BOM."],
                &[],
            ],
            ..Canary::new("bom", format!("\u{feff}{}", described("bom", "A skill.")))
        },
        Canary {
            shared_warnings: &[YAML_COMMENT],
            ..Canary::new(
                "yaml-comment",
                described("yaml-comment", "Some text # hidden"),
            )
        },
        Canary::new(
            "folded",
            described("folded", ">\n  A folded description\n  on two lines."),
        ),
        Canary::new(
            "multiline-quoted",
            described(
                "multiline-quoted",
                "\"A quoted description\n  on two lines.\"",
            ),
        ),
        Canary::new("crlf", described("crlf", "A skill.").replace('\n', "\r\n")),
        Canary {
            host_warnings: [
                &[
                    "Claude Code uses the first body line as the description because SKILL.md has no YAML frontmatter",
                ],
                &["Codex does not show this skill because SKILL.md has no YAML frontmatter"],
                &[
                    "Antigravity CLI does not show this skill because SKILL.md has no YAML frontmatter",
                ],
            ],
            ..Canary::new("no-frontmatter", "Body first line.\n".to_owned())
        },
        Canary {
            shared_warnings: &[
                "my-skills could not read the YAML frontmatter.",
                MISSING_LINK,
            ],
            ..Canary::new(
                "invalid-yaml",
                described("invalid-yaml", "[unclosed") + "[Missing](missing.md)\n",
            )
        },
        Canary {
            host_warnings: [
                &[
                    "description and when_to_use combined (1537 characters)",
                    TRUNCATED[0],
                ],
                &["Codex ignores these YAML fields: when_to_use."],
                &["Antigravity CLI ignores these YAML fields: when_to_use."],
            ],
            ..Canary::new(
                "claude-combined",
                skill(format!(
                    "name: claude-combined\ndescription: {}\nwhen_to_use: {}",
                    "가".repeat(767),
                    "나".repeat(770)
                )),
            )
        },
    ]
}

fn with_canaries(canaries: &[Canary]) -> Fixture {
    let fixture = Fixture::repo();
    let mut manifest = fs::read_to_string(fixture.manifest()).unwrap();
    manifest = manifest.replace(
        "[targets.agy]\nenabled = false",
        "[targets.agy]\nenabled = true",
    );
    for canary in canaries {
        fs::create_dir(fixture.canonical(canary.name)).unwrap();
        fs::write(
            fixture.canonical(canary.name).join("SKILL.md"),
            &canary.content,
        )
        .unwrap();
        manifest.push_str(&format!("\n[skills.{}]\n", canary.name));
    }
    fs::write(fixture.manifest(), manifest).unwrap();
    fixture
}

fn assert_warnings(row: &Value, expected: &[&str], label: &str) {
    assert_eq!(row["ok"], true, "{label}: {row}");
    assert_eq!(row["errors"], json!([]), "{label}: {row}");
    let warnings = row["warnings"].as_array().unwrap();
    for expected in expected {
        assert!(
            warnings
                .iter()
                .any(|w| w.as_str().unwrap().contains(expected)),
            "{label}: missing {expected:?} in {warnings:?}"
        );
    }
    for warning in warnings {
        assert!(
            expected
                .iter()
                .any(|expected| warning.as_str().unwrap().contains(expected)),
            "{label}: unexpected {warning}"
        );
    }
}

#[test]
fn validate_reports_canary_display_for_only_the_selected_host() {
    let canaries = canaries();
    let fixture = with_canaries(&canaries);
    for (index, host) in HOSTS.into_iter().enumerate() {
        let result = support_h::json(
            fixture
                .command()
                .args(["validate", "--host", host, "--json"]),
        );
        assert_eq!(result["ok"], true, "{result}");
        for canary in &canaries {
            let row = result["skills"]
                .as_array()
                .unwrap()
                .iter()
                .find(|row| row["skill"] == canary.name)
                .unwrap();
            let expected = [canary.shared_warnings, canary.host_warnings[index]].concat();
            assert_warnings(row, &expected, &format!("{host} / {}", canary.name));
        }
    }
}

#[test]
fn sync_and_install_connect_every_canary_and_preserve_its_bytes() {
    let canaries = canaries();
    for args in [&["sync"][..], &["install", "--mode", "copy"][..]] {
        let fixture = with_canaries(&canaries);
        let command = args[0];
        let result = support_h::json(fixture.command().args(args).args(["--yes", "--json"]));
        assert_eq!(result["blocked"], false, "{result}");
        assert_eq!(result["applied"], true, "{result}");
        for canary in &canaries {
            let rows = result["validation"].as_array().unwrap();
            let standard = rows
                .iter()
                .find(|row| row["skill"] == canary.name && row["host"].is_null())
                .unwrap();
            assert_warnings(standard, canary.shared_warnings, canary.name);
            for (index, host) in HOSTS.into_iter().enumerate() {
                let installed = fixture.host(host).join(canary.name).join("SKILL.md");
                let label = format!("{command} / {host} / {}", canary.name);
                assert_eq!(
                    fs::read(installed).unwrap(),
                    canary.content.as_bytes(),
                    "{label}"
                );
                assert_eq!(
                    fs::read(fixture.canonical(canary.name).join("SKILL.md")).unwrap(),
                    canary.content.as_bytes(),
                    "{label}"
                );
                let row = rows
                    .iter()
                    .find(|row| row["skill"] == canary.name && row["host"] == host);
                if let Some(row) = row {
                    assert_warnings(row, canary.host_warnings[index], &label);
                } else {
                    assert!(canary.host_warnings[index].is_empty(), "{label}");
                }
            }
        }
    }
}
