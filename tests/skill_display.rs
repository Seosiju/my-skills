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
    let quoted = |text: String| serde_json::to_string(&text).unwrap();
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
                &["Claude Code lists this skill as \"long-name\" using the folder name."],
                &["Codex does not show this skill because its name is 70 characters. If you want it to appear, use a name of 64 characters or fewer."],
                &["Antigravity CLI lists this skill as \"xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\" using the YAML name."],
            ],
            ..Canary::new("long-name", described(&"x".repeat(70), "A skill."))
        },
        Canary::new("desc-800", described("desc-800", &"가".repeat(800))),
        Canary {
            host_warnings: [&[], &["Codex shows a shortened description (1100 characters). It keeps the first 1021 characters and adds \"...\"."], &[]],
            ..Canary::new("desc-1100", described("desc-1100", &"가".repeat(1100)))
        },
        Canary {
            host_warnings: [
                &["Claude Code shows a shortened description (2000 characters). It keeps the first 1535 characters and adds \"…\"."],
                &["Codex shows a shortened description (2000 characters). It keeps the first 1021 characters and adds \"...\"."],
                &[],
            ],
            ..Canary::new("desc-2000", described("desc-2000", &"가".repeat(2000)))
        },
        Canary::new(
            "desc-trimmed",
            described("desc-trimmed", &quoted(format!("{}{}{}", " ".repeat(100), "가".repeat(1000), " ".repeat(100)))),
        ),
        Canary::new(
            "desc-folded-edge",
            described("desc-folded-edge", &quoted(format!("{}\t\u{a0} \n{}", "가".repeat(512), "나".repeat(511)))),
        ),
        Canary {
            host_warnings: [&[], &["Codex shows a shortened description (1025 characters). It keeps the first 1021 characters and adds \"...\"."], &[]],
            ..Canary::new(
                "desc-folded-over",
                described("desc-folded-over", &quoted(format!("{}\t\u{a0} \n{}", "가".repeat(512), "나".repeat(512)))),
            )
        },
        Canary {
            shared_warnings: &["Referenced supporting file not found: actual-missing.md."],
            ..Canary::new(
                "fence-indent",
                described("fence-indent", "A link diagnostic check.")
                    + "~~~md\n    ~~~\n[Inside code](false-positive.md)\n~~~\n\n[Outside code](actual-missing.md)\n",
            )
        },
        Canary {
            shared_warnings: &[
                "Referenced supporting file not found: list-missing.md.",
                "Referenced supporting file not found: end-missing.md.",
            ],
            ..Canary::new(
                "list-fence-indent",
                described("list-fence-indent", "A fenced block inside a list.")
                    + "- item\n\n  ~~~md\n  [Inside](inside.md)\n     ~~~\n\n  [Missing](list-missing.md)\n\n[End](end-missing.md)\n",
            )
        },
        Canary {
            shared_warnings: &["Referenced supporting file not found: after-nested-missing.md."],
            ..Canary::new(
                "nested-fence-indent",
                described("nested-fence-indent", "A fenced block inside a nested list.")
                    + "- outer\n  - inner\n\n    ```md\n    [InNested](in-nested.md)\n    ```\n\n[AfterNested](after-nested-missing.md)\n",
            )
        },
        Canary {
            shared_warnings: &[
                "Referenced supporting file not found: notes/(guide).md?raw=1#intro.",
                "Referenced supporting file not found: images/report chart.png.",
            ],
            ..Canary::new(
                "reference-links",
                described("reference-links", "Reference links and images.")
                    + "\n[Guide][docs] and ![Chart][fig]\n\n[docs]: notes/(guide).md?raw=1#intro \"Guide\"\n[fig]: <images/report chart.png> \"Chart\"\n",
            )
        },
        Canary {
            host_warnings: [
                &[],
                &["Codex ignores these YAML fields: allowed-tools, argument-hint, model, paths."],
                &["Antigravity CLI ignores these YAML fields: allowed-tools, argument-hint, model, paths."],
            ],
            ..Canary::new("claude-fields", skill("name: claude-fields\ndescription: A skill.\nallowed-tools: Read\nargument-hint: '[value]'\nmodel: sonnet\npaths: ['docs/**']".into()))
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
            host_warnings: [
                &["If Claude Code cannot read this YAML, it loads the skill without those fields and uses the first body line as the description"],
                &["Codex may not show this skill if it cannot read the YAML frontmatter."],
                &["Antigravity CLI may not show this skill if it cannot read the YAML frontmatter."],
            ],
            ..Canary::new(
                "invalid-yaml",
                described("invalid-yaml", "[unclosed") + "[Missing](missing.md)\n",
            )
        },
        Canary {
            shared_warnings: &["unterminated frontmatter: no closing '---' delimiter found"],
            host_warnings: [
                &["Claude Code may treat SKILL.md as body text because the YAML frontmatter has no closing '---'."],
                &["Codex does not show this skill because the YAML frontmatter has no closing '---'."],
                &["Antigravity CLI does not show this skill because the YAML frontmatter has no closing '---'."],
            ],
            ..Canary::new("unterminated", "---\nname: unterminated\ndescription: A skill.\n".into())
        },
        Canary {
            host_warnings: [
                &["Claude Code may display the description differently or omit the skill when the YAML description is a list or mapping."],
                &["Codex does not show this skill because its YAML description is a list or mapping."],
                &["Antigravity CLI may display the description differently or omit the skill when the YAML description is a list or mapping."],
            ],
            ..Canary::new("list-description", described("list-description", "[one, two]"))
        },
        Canary {
            host_warnings: [
                &["Claude Code may display the description differently or omit the skill when the YAML description is a list or mapping."],
                &["Codex may display the description differently or omit the skill when the YAML description is a list or mapping."],
                &["Antigravity CLI may display the description differently or omit the skill when the YAML description is a list or mapping."],
            ],
            ..Canary::new("repairable-description", described("repairable-description", "{part: text}"))
        },
        Canary {
            host_warnings: [
                &[
                    "Claude Code shows a shortened description and when_to_use combined (1537 characters). It keeps the first 1535 characters and adds \"…\".",
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
    assert_eq!(warnings.len(), expected.len(), "{label}: {warnings:?}");
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
fn validate_uses_the_manifest_hosts_without_an_override() {
    let canaries = canaries();
    let fixture = with_canaries(&canaries);
    let manifest = fs::read_to_string(fixture.manifest()).unwrap().replace(
        "[skills.desc-2000]\n",
        "[skills.desc-2000]\nhosts = ['claude']\n",
    );
    fs::write(fixture.manifest(), manifest).unwrap();
    let result = support_h::json(fixture.command().args(["validate", "desc-2000", "--json"]));
    assert_warnings(
        &result["skills"][0],
        &["Claude Code shows a shortened description (2000 characters)."],
        "claude-only skill",
    );
}

#[test]
fn codex_checks_the_length_of_a_drafts_fallback_folder_name() {
    let fixture = with_canaries(&[]);
    let name = "a".repeat(65);
    let dir = fixture.canonical(&name);
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("SKILL.md"), "---\ndescription: A skill.\n---\n").unwrap();
    let result = support_h::json(fixture.command().args(["validate", &name, "--json"]));
    assert_eq!(
        result["skills"][0],
        json!({
            "skill": name,
            "ok": true,
            "errors": [],
            "warnings": ["Codex does not show this skill because it uses the 65-character folder name. If you want it to appear, use a folder name or YAML name of 64 characters or fewer."]
        })
    );
}

#[test]
fn codex_warns_about_collection_names_and_preserves_plain_scalar_names() {
    let rejected = "Codex does not show this skill because its YAML name is a list or mapping. If you want it to appear, use a plain text name.";
    let uncertain = "Codex may display the name differently or omit the skill when the YAML name is a list or mapping. If you want a predictable name, use a plain text name.";
    let cases = [
        ("name-list", "name: [one, two]", rejected),
        ("name-mapping", "name:\n  part: text", rejected),
        ("name-may-repair", "name: [one, {part: text}]", uncertain),
        (
            "name-bool",
            "name: false",
            "Codex lists this skill as \"false\" using the YAML name. The folder is \"name-bool\" and the YAML name is \"false\". If you want the same name across services, make the two names match.",
        ),
        (
            "name-octal",
            "name: 0123",
            "Codex lists this skill as \"0123\" using the YAML name. The folder is \"name-octal\" and the YAML name is \"0123\". If you want the same name across services, make the two names match.",
        ),
        (
            "name-anchored",
            "metadata: {flag: &flag false}\nname: *flag",
            "Codex lists this skill as \"false\" using the YAML name. The folder is \"name-anchored\" and the YAML name is \"false\". If you want the same name across services, make the two names match.",
        ),
    ];
    let canaries = cases.map(|(name, yaml, _)| {
        Canary::new(name, format!("---\n{yaml}\ndescription: A skill.\n---\n"))
    });
    let fixture = with_canaries(&canaries);
    let result = support_h::json(
        fixture
            .command()
            .args(["validate", "--host", "codex", "--json"]),
    );
    assert_eq!(result["ok"], true);
    let rows = result["skills"].as_array().unwrap();
    for (name, _, expected) in cases {
        let row = rows.iter().find(|row| row["skill"] == name).unwrap();
        assert_eq!(row["errors"], json!([]), "{name}");
        assert_eq!(row["warnings"], json!([expected]), "{name}");
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
