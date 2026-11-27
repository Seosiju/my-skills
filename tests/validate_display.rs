//! Frontmatter parsing, validation goldens, and measured host display canaries.

mod common;

mod frontmatter_parity {
    use std::collections::BTreeMap;
    use std::fs;

    use my_skills::check::frontmatter::{Value, parse_frontmatter};
    use my_skills::check::validate::validate_skill_for_host;
    use my_skills::drive::hosts::AGY;

    #[test]
    fn matches_delimiters_bom_and_unicode_line_separators() {
        let text = "\u{feff}  --- \r\nname: example\r\ndescription: metadata\r\n  ---\t\r\n\r\n# Body\u{2028}text\r\n\r\n";
        let (metadata, body) = parse_frontmatter(text).unwrap();
        assert_eq!(
            metadata,
            BTreeMap::from([
                (
                    "description".to_owned(),
                    Value::String("metadata".to_owned())
                ),
                ("name".to_owned(), Value::String("example".to_owned())),
            ]),
        );
        assert_eq!(body, "\n# Body\ntext\n");
        assert_eq!(parse_frontmatter("---\r---\rbody").unwrap().1, "body");
        assert_eq!(parse_frontmatter("---\n---\n---").unwrap().1, "---");
        assert_eq!(parse_frontmatter("---\n---").unwrap().1, "");
    }

    #[test]
    fn matches_structural_failures_and_empty_blocks() {
        for (input, expected) in [
            ("", "file is empty: no frontmatter found"),
            (
                "\u{feff}",
                "missing opening '---': frontmatter must start on the first line",
            ),
            (
                "\n---\n---",
                "missing opening '---': frontmatter must start on the first line",
            ),
            (
                "--- # comment\n---",
                "missing opening '---': frontmatter must start on the first line",
            ),
            (
                "---\nname: example\n--- # comment",
                "unterminated frontmatter: no closing '---' delimiter found",
            ),
        ] {
            assert_eq!(parse_frontmatter(input).unwrap_err().0, expected);
        }
        for input in [
            "---\n---",
            "---\n# comment\n---",
            "---\n~\n---",
            "---\nnull\n---",
            "---\n{}\n---",
        ] {
            assert_eq!(
                parse_frontmatter(input).unwrap(),
                (BTreeMap::new(), String::new())
            );
        }
    }

    #[test]
    fn resolves_yaml_12_core_values_and_keeps_last_duplicate() {
        let (metadata, _) = parse_frontmatter(
        "---\nname: bad\n'name': good\ndescription: [true, False, null, ~, yes, No, ON, Off, 0123, 0o123, 1:20, 1e3, 1.5, 2020-01-02]\n---",
    )
    .unwrap();

        assert_eq!(metadata["name"], Value::String("good".into()));
        assert_eq!(
            metadata["description"],
            Value::List(vec![
                Value::Bool(true),
                Value::Bool(false),
                Value::Null,
                Value::Null,
                Value::String("yes".into()),
                Value::String("No".into()),
                Value::String("ON".into()),
                Value::String("Off".into()),
                Value::Int(123),
                Value::String("0o123".into()),
                Value::String("1:20".into()),
                Value::Float(1000.0),
                Value::Float(1.5),
                Value::String("2020-01-02".into()),
            ])
        );
    }

    #[test]
    fn preserves_large_integers_verbatim_in_public_value() {
        let big = "123456789012345678901234567890";
        let signed_padded = "+00123456789012345678901234567890";
        let (metadata, _) = parse_frontmatter(&format!(
            "---\nbig: {big}\nsigned: {signed_padded}\nsmall: -42\n---"
        ))
        .unwrap();
        assert_eq!(metadata["big"], Value::String(big.into()));
        assert_eq!(metadata["signed"], Value::String(signed_padded.into()));
        assert_eq!(metadata["small"], Value::Int(-42));
    }

    #[test]
    fn enforces_nesting_depth_limit_of_64_without_stack_overflow() {
        let within = format!("---\na: {}1{}\n---", "[".repeat(63), "]".repeat(63));
        assert!(parse_frontmatter(&within).is_ok());

        let exceeded = format!("---\na: {}1{}\n---", "[".repeat(64), "]".repeat(64));
        assert_eq!(
            parse_frontmatter(&exceeded).unwrap_err().0,
            "malformed YAML in frontmatter: frontmatter nesting limit exceeded"
        );

        let deep_flow = format!("---\na: {}1{}\n---", "[".repeat(2000), "]".repeat(2000));
        assert_eq!(
            parse_frontmatter(&deep_flow).unwrap_err().0,
            "malformed YAML in frontmatter: frontmatter nesting limit exceeded"
        );

        let mut deep_block = String::from("---\n");
        for level in 0..2000 {
            for _ in 0..level {
                deep_block.push_str("  ");
            }
            deep_block.push_str("a:\n");
        }
        deep_block.push_str("---");
        assert_eq!(
            parse_frontmatter(&deep_block).unwrap_err().0,
            "malformed YAML in frontmatter: frontmatter nesting limit exceeded"
        );
    }

    #[test]
    fn rejects_alias_depth_bypass_and_tree_explosion() {
        let mut chain = String::from("---\na0: &a0 [1]\n");
        for i in 1..=64 {
            chain.push_str(&format!("a{i}: &a{i} [*a{}]\n", i - 1));
        }
        chain.push_str("---");
        assert_eq!(
            parse_frontmatter(&chain).unwrap_err().0,
            "malformed YAML in frontmatter: frontmatter nesting limit exceeded"
        );

        let mut bomb = String::from("---\na0: &a0 [x, x]\n");
        for i in 1..=20 {
            bomb.push_str(&format!("a{i}: &a{i} [*a{}, *a{}]\n", i - 1, i - 1));
        }
        bomb.push_str("---");
        assert_eq!(
            parse_frontmatter(&bomb).unwrap_err().0,
            "malformed YAML in frontmatter: frontmatter size limit exceeded"
        );

        let payload = "x".repeat(16 * 1024);
        let mut byte_bomb = format!("---\nbig: &big {payload}\nrefs: [");
        for i in 0..70 {
            if i > 0 {
                byte_bomb.push_str(", ");
            }
            byte_bomb.push_str("*big");
        }
        byte_bomb.push_str("]\n---");
        assert_eq!(
            parse_frontmatter(&byte_bomb).unwrap_err().0,
            "malformed YAML in frontmatter: frontmatter size limit exceeded"
        );

        let long_int_key = format!("{}1", "0".repeat(16 * 1024));
        let mut dup_key_bomb = format!("---\nm: &m {{{long_int_key}: a, 1: b}}\nrefs: [");
        for i in 0..65 {
            if i > 0 {
                dup_key_bomb.push_str(", ");
            }
            dup_key_bomb.push_str("*m");
        }
        dup_key_bomb.push_str("]\n---");
        assert_eq!(
            parse_frontmatter(&dup_key_bomb).unwrap_err().0,
            "malformed YAML in frontmatter: frontmatter size limit exceeded"
        );
    }

    #[test]
    fn allows_large_shallow_document_without_aliases() {
        let mut doc = String::from("---\nitems: [");
        for i in 0..10_005 {
            if i > 0 {
                doc.push(',');
            }
            doc.push_str(&i.to_string());
        }
        doc.push_str("]\n---");
        let (metadata, _) = parse_frontmatter(&doc).unwrap();
        let Value::List(items) = &metadata["items"] else {
            panic!("expected list");
        };
        assert_eq!(items.len(), 10_005);
    }

    #[test]
    fn deduplicates_equivalent_integer_keys_last_wins() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join("int-keys");
        fs::create_dir(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: {001: a, +1: b, 1: c, -0: d, +00: e}\ndescription: int keys\n---\n",
        )
        .unwrap();
        let result = validate_skill_for_host(&skill_dir, &AGY);
        assert!(result.ok(), "{result:?}");
        assert_eq!(
            result.warnings,
            [
                "Antigravity CLI lists this skill as \"{001: c, -0: e}\" using the YAML name. The folder is \"int-keys\" and the YAML name is \"{001: c, -0: e}\". If you want the same name across services, make the two names match."
            ]
        );
    }

    #[test]
    fn preserves_distinct_key_types_in_internal_mapping() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join("distinct-keys");
        fs::create_dir(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: {1: integer, '1': text}\ndescription: distinct keys\n---\n",
        )
        .unwrap();
        let result = validate_skill_for_host(&skill_dir, &AGY);
        assert!(result.ok(), "{result:?}");
        assert_eq!(
            result.warnings,
            [
                "Antigravity CLI lists this skill as \"{1: integer, 1: text}\" using the YAML name. The folder is \"distinct-keys\" and the YAML name is \"{1: integer, 1: text}\". If you want the same name across services, make the two names match."
            ]
        );
    }

    #[test]
    fn supports_verbatim_and_split_prefix_yaml_core_tag_uris() {
        let (metadata, _) = parse_frontmatter(
        "---\nnum: !<tag:yaml.org,2002:int> 42\nmap: !<tag:yaml.org,2002:map> {value: 42}\nseq: !<tag:yaml.org,2002:seq> [1, 2]\n---",
    )
    .unwrap();
        assert_eq!(metadata["num"], Value::Int(42));
        assert_eq!(
            metadata["map"],
            Value::Map(BTreeMap::from([("value".to_owned(), Value::Int(42))]))
        );
        assert_eq!(
            metadata["seq"],
            Value::List(vec![Value::Int(1), Value::Int(2)])
        );

        let (split_tag, _) =
            parse_frontmatter("---\n%TAG !core! tag:yaml.org,2002:i\n--- {num: !core!nt 42}\n---")
                .unwrap();
        assert_eq!(split_tag["num"], Value::Int(42));
    }

    #[test]
    fn rejects_non_mapping_root_multi_document_missing_alias_and_bad_tags() {
        for (input, expected) in [
            (
                "---\n- a\n- b\n---",
                "frontmatter must be a YAML mapping, got list",
            ),
            (
                "---\ntrue\n---",
                "frontmatter must be a YAML mapping, got bool",
            ),
            (
                "---\n42\n---",
                "frontmatter must be a YAML mapping, got int",
            ),
            (
                "---\n1.5\n---",
                "frontmatter must be a YAML mapping, got float",
            ),
            (
                "---\nhello\n---",
                "frontmatter must be a YAML mapping, got str",
            ),
            (
                "---\na: 1\n...\n--- {b: 2}\n---",
                "malformed YAML in frontmatter: expected a single YAML document",
            ),
        ] {
            assert_eq!(parse_frontmatter(input).unwrap_err().0, expected, "{input}");
        }

        for bad in [
            "---\ndescription: *nope\n---",
            "---\ndescription: !!int abc\n---",
            "---\ndescription: !unknown text\n---",
            "---\ndescription: !!binary SGVsbG8=\n---",
        ] {
            let err = parse_frontmatter(bad).unwrap_err().0;
            assert!(
                err.starts_with("malformed YAML in frontmatter:"),
                "{bad}: {err}"
            );
        }
    }
}

mod validate_golden {
    //! Exact snapshots of current Rust diagnostics. The v0.4.1 Python oracle
    //! was removed when the validation contract changed.

    use std::collections::{BTreeMap, BTreeSet};
    use std::fs;
    use std::path::Path;

    use my_skills::check::validate::{
        ValidationResult, validate_name, validate_skill, validate_skill_for_host,
    };
    use my_skills::drive::hosts::{ALL, CLAUDE, DescriptionLimit, SkillDisplay};
    use serde::Deserialize;

    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct Diagnostics {
        errors: Vec<String>,
        warnings: Vec<String>,
    }

    #[test]
    fn matches_validation_golden_for_every_fixture_and_builtin_host() {
        let golden: BTreeMap<String, BTreeMap<String, Diagnostics>> =
            serde_json::from_str(include_str!("golden/validate/validation.json")).unwrap();
        let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let fixtures = fixture_root.join("validate");
        let shared = fixture_root.join("shared");
        let temp = tempfile::tempdir().unwrap();
        let expected_hosts: BTreeSet<_> = ALL
            .iter()
            .map(|host| host.name)
            .chain(["standard"])
            .collect();

        for (name, results) in &golden {
            let fixture = fixtures.join(name);
            let source = if fixture.is_dir() {
                fixture
            } else {
                shared.join(name)
            };
            let path = temp.path().join(name);
            copy_fixture(&source, &path);
            assert_eq!(
                results.keys().map(String::as_str).collect::<BTreeSet<_>>(),
                expected_hosts
            );

            assert_diagnostics(
                name,
                "standard",
                &path,
                validate_skill(&path),
                &results["standard"],
            );
            for host in ALL {
                assert_diagnostics(
                    name,
                    host.name,
                    &path,
                    validate_skill_for_host(&path, host),
                    &results[host.name],
                );
            }
        }

        for entry in fs::read_dir(fixtures).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                assert!(
                    golden.contains_key(entry.file_name().to_str().unwrap()),
                    "fixture missing from golden: {:?}",
                    entry.path()
                );
            }
        }
    }

    fn copy_fixture(source: &Path, destination: &Path) {
        fs::create_dir(destination).unwrap();
        for entry in fs::read_dir(source).unwrap() {
            let entry = entry.unwrap();
            let path = destination.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_fixture(&entry.path(), &path);
            } else {
                fs::copy(entry.path(), path).unwrap();
            }
        }
    }

    fn assert_diagnostics(
        name: &str,
        host: &str,
        path: &Path,
        result: ValidationResult,
        expected: &Diagnostics,
    ) {
        assert_eq!(result.skill, name);
        assert_eq!(result.path, path);
        assert_eq!(result.ok(), expected.errors.is_empty(), "{name} / {host}");
        let diagnostics = Diagnostics {
            errors: result.errors,
            warnings: result.warnings,
        };
        assert_eq!(&diagnostics, expected, "{name} / {host}");
    }

    #[test]
    fn name_rules_match_python_regex_and_count_code_points() {
        let format_error = "name must be lowercase alphanumeric words separated by single hyphens (no leading, trailing, or consecutive hyphens)";
        for name in ["a", "a1-b2-c3", "012", "a-b\n", &"a".repeat(64)] {
            assert!(validate_name(name).is_empty(), "{name:?}");
        }
        for name in [
            "",
            "Bad",
            "-lead",
            "trail-",
            "double--hyphen",
            "a_b",
            "a\r\n",
            "a\n\n",
            "é",
        ] {
            assert_eq!(validate_name(name), [format_error], "{name:?}");
        }
        assert_eq!(validate_name(&"가".repeat(64)), [format_error]);
        assert_eq!(
            validate_name(&"가".repeat(65)),
            ["name exceeds 64 characters", format_error]
        );
    }

    #[test]
    fn plain_scalars_warn_about_inline_yaml_comments() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tiny-skill");
        fs::create_dir(&path).unwrap();
        for (yaml, field, preview) in [
            (
                "name: tiny-skill\ndescription: Gets cut off # lost text",
                "description",
                "Gets cut off",
            ),
            (
                "name: tiny-skill # lost suffix\ndescription: An example",
                "name",
                "tiny-skill",
            ),
            (
                "name: tiny-skill\r\n'description': 설명 ✏️\r\n  on a second line\t# lost text",
                "description",
                "설명 ✏️ on a second line",
            ),
            (
                "{name: tiny-skill, description: Shows C# # lost text\n}",
                "description",
                "Shows C#",
            ),
            (
                "name: tiny-skill\nmetadata: {summary: &summary An example # lost text\n}\ndescription: *summary",
                "description",
                "An example",
            ),
        ] {
            fs::write(path.join("SKILL.md"), format!("---\n{yaml}\n---\n")).unwrap();
            let result = validate_skill(&path);
            assert!(result.ok(), "{result:?}");
            assert_eq!(
                result.warnings,
                [format!(
                    "A YAML comment cuts off '{field}'. Agents see {preview:?}. If you want the full value to appear, put it in quotes."
                )],
                "{yaml:?}"
            );
        }
    }

    #[test]
    fn intact_scalars_and_standalone_comments_do_not_warn() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tiny-skill");
        fs::create_dir(&path).unwrap();
        for fields in [
            "description: 'Keeps # fragment'",
            "description: \"Keeps # fragment\" # note after quoted value",
            "description: Shows C# and topic#tag",
            "description: |\n  Keeps # fragment",
            "description: > # note after block header\n  Keeps # fragment",
            "description: Keeps the value\n  # standalone comment",
            "description: #tag\n  Keeps the value",
            "description: Keeps the value\nmetadata:\n  description: Nested # comment",
            "description: Overridden # comment\ndescription: 'Keeps # fragment'",
        ] {
            fs::write(
                path.join("SKILL.md"),
                format!("---\nname: tiny-skill\n{fields}\n---\n"),
            )
            .unwrap();
            let result = validate_skill(&path);
            assert!(
                result.ok() && result.warnings.is_empty(),
                "{fields:?}: {result:?}"
            );
        }
    }

    #[test]
    fn host_configuration_controls_limits_and_kept_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tiny-skill");
        fs::create_dir(&path).unwrap();
        fs::write(
        path.join("SKILL.md"),
        "---\nname: tiny-skill\ndescription: 가나\ncustom: yes\nother: no\nlicense: MIT\ncompatibility: Terminal\nmetadata: {version: '1'}\n---\n",
    )
    .unwrap();
        let host = my_skills::drive::hosts::HostConfig {
            name: "tiny",
            display_name: "Tiny",
            skill_display: SkillDisplay {
                description_limit: Some(DescriptionLimit {
                    max_chars: 1,
                    suffix: "…",
                    extra_field: None,
                }),
                frontmatter_keep_fields: &[
                    "name",
                    "description",
                    "custom",
                    "license",
                    "compatibility",
                    "metadata",
                ],
                ..CLAUDE.skill_display
            },
            ..CLAUDE
        };
        let result = validate_skill_for_host(&path, &host);
        assert!(result.ok(), "{result:?}");
        assert_eq!(
            result.warnings,
            [
                "Tiny shows a shortened description (2 characters). It keeps the first 0 characters and adds \"…\". If you want the full text to appear, keep it to 1 characters or fewer.",
                "Tiny ignores these YAML fields: other. If you want Tiny to see that information, put it in a supported field or the skill body."
            ]
        );
    }
}

mod skill_display {
    use std::fs;

    use crate::common::Fixture;
    use serde_json::{Value, json};

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
        let described =
            |name, description| skill(format!("name: {name}\ndescription: {description}"));
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
            shared_warnings: &["Referenced supporting file not found: assets/local-missing.md."],
            ..Canary::new(
                "external-links",
                described("external-links", "URI links and local links.")
                    + "\n<user@example.com> <ftp://example.com/remote.txt>\n\n[Remote](custom+git.v2-data:some/resource)\n![Image](data:image/png;base64,AAAA)\n[Drive](C:/missing.txt)\n[Local](assets/local-missing.md)\n",
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
                &["If Claude Code cannot parse the YAML frontmatter, it loads the skill body without the YAML fields. If you want it to use those fields, fix the YAML."],
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
                &["The YAML description is a list or mapping. Claude Code uses the first non-empty body line when a description is omitted. If you want a custom description, write the YAML value as text."],
                &["Codex does not show this skill because its YAML description is a list or mapping."],
                &["Antigravity CLI may display the description differently or omit the skill when the YAML description is a list or mapping."],
            ],
            ..Canary::new("list-description", described("list-description", "[one, two]"))
        },
        Canary {
            host_warnings: [
                &["The YAML description is a list or mapping. Claude Code uses the first non-empty body line when a description is omitted. If you want a custom description, write the YAML value as text."],
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
            let result = crate::common::json(
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
        let result =
            crate::common::json(fixture.command().args(["validate", "desc-2000", "--json"]));
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
        let result = crate::common::json(fixture.command().args(["validate", &name, "--json"]));
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
        let result = crate::common::json(
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
    fn codex_collection_repair_uses_the_effective_value_of_each_field() {
        let rejects_name = "Codex does not show this skill because its YAML name is a list or mapping. If you want it to appear, use a plain text name.";
        let rejects_description = "Codex does not show this skill because its YAML description is a list or mapping. If you want it to appear, use a plain text description.";
        let may_repair_name = "Codex may display the name differently or omit the skill when the YAML name is a list or mapping. If you want a predictable name, use a plain text name.";
        let cases = [
            (
                "name-context",
                "name: [one, two]\ndescription: A skill.\nextra: 'Unrelated: text'",
                &[rejects_name][..],
            ),
            (
                "desc-context",
                "name: desc-context\ndescription: [one, two]\nextra: 'Unrelated: text'",
                &[rejects_description][..],
            ),
            (
                "name-inline",
                "{name: [one, two], description: A skill.}",
                &[rejects_name][..],
            ),
            (
                "name-comment",
                "name: [one, two] # Unrelated: text\ndescription: A skill.",
                &[rejects_name][..],
            ),
            (
                "repair-desc",
                "name: [one, two]\ndescription: {part: text}",
                &[rejects_name][..],
            ),
            (
                "repair-name",
                "name: [one, {part: text}]\ndescription: [one, two]",
                &[may_repair_name, rejects_description][..],
            ),
            (
                "name-alias",
                "metadata:\n  label: &label [one, {part: text}]\nname: *label\ndescription: A skill.",
                &[may_repair_name][..],
            ),
        ];
        let canaries =
            cases.map(|(name, yaml, _)| Canary::new(name, format!("---\n{yaml}\n---\n")));
        let fixture = with_canaries(&canaries);
        let result = crate::common::json(
            fixture
                .command()
                .args(["validate", "--host", "codex", "--json"]),
        );
        assert_eq!(result["ok"], true);
        let rows = result["skills"].as_array().unwrap();
        for (name, _, expected) in cases {
            let row = rows.iter().find(|row| row["skill"] == name).unwrap();
            assert_eq!(row["errors"], json!([]), "{name}");
            assert_eq!(row["warnings"], json!(expected), "{name}");
        }
    }

    #[test]
    fn agy_distinguishes_non_text_scalars_from_a_missing_description() {
        let canaries = [
            ("bool-desc", "false"),
            ("zero-desc", "0"),
            ("null-desc", "null"),
        ]
        .map(|(name, value)| {
            Canary::new(
                name,
                format!("---\nname: {name}\ndescription: {value}\n---\n"),
            )
        });
        let fixture = with_canaries(&canaries);
        let result = crate::common::json(
            fixture
                .command()
                .args(["validate", "--host", "agy", "--json"]),
        );
        assert_eq!(result["ok"], true);
        for row in result["skills"].as_array().unwrap() {
            let expected = if row["skill"] == "null-desc" {
                "Antigravity CLI shows an empty description because the YAML description is missing or empty. If you want a custom description, add one to the YAML frontmatter."
            } else {
                "The YAML description is a non-text scalar. How Antigravity CLI displays it may vary. If you want a text description, put the value in quotes."
            };
            assert_eq!(row["errors"], json!([]), "{row}");
            assert_eq!(row["warnings"], json!([expected]), "{row}");
        }
    }

    #[test]
    fn sync_and_install_connect_every_canary_and_preserve_its_bytes() {
        let canaries = canaries();
        for args in [&["sync"][..], &["sync", "--mode", "copy"][..]] {
            let fixture = with_canaries(&canaries);
            let command = args[0];
            let result =
                crate::common::json(fixture.command().args(args).args(["--yes", "--json"]));
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
}
