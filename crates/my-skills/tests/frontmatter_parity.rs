use std::collections::BTreeMap;

use my_skills::frontmatter::{Value, parse_frontmatter};

#[test]
fn matches_delimiters_bom_and_python_splitlines() {
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
        "---\n{}\n---",
    ] {
        assert_eq!(
            parse_frontmatter(input).unwrap(),
            (BTreeMap::new(), String::new())
        );
    }
}

#[test]
fn resolves_yaml_11_values_and_keeps_last_duplicate() {
    let (metadata, _) = parse_frontmatter(
        "---\n        name: bad\n        'name': good\n        description: [yes, No, ON, Off, 'off', yEs, 0123, 0o123, 1:20, 1e3, 1.0e+3, 2020-01-02]\n        ---",
    ).unwrap();

    assert_eq!(metadata["name"], Value::String("good".into()));
    assert_eq!(
        metadata["description"],
        Value::List(vec![
            Value::Bool(true),
            Value::Bool(false),
            Value::Bool(true),
            Value::Bool(false),
            Value::String("off".into()),
            Value::String("yEs".into()),
            Value::Int(83),
            Value::String("0o123".into()),
            Value::Int(80),
            Value::String("1e3".into()),
            Value::Float(1000.0),
            Value::String("2020-01-02".into()),
        ])
    );
}
