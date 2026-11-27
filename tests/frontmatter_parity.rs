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
