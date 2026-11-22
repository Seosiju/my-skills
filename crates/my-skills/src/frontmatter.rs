//! SKILL.md YAML frontmatter. Stream D. Mirrors v0.4.1 `frontmatter.py`.

use std::collections::BTreeMap;

/// A frontmatter scalar or nested value, kept generic so validation can
/// report type errors the way v0.4.1 does.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    List(Vec<Value>),
    Map(BTreeMap<String, Value>),
}

/// Parse failure. The message text mirrors v0.4.1 `FrontmatterError`;
/// validation turns it into an error string.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct FrontmatterError(pub String);

/// Split `text` into (frontmatter map, body), as v0.4.1 `parse_frontmatter`.
pub fn parse_frontmatter(
    _text: &str,
) -> std::result::Result<(BTreeMap<String, Value>, String), FrontmatterError> {
    todo!("stream D")
}
