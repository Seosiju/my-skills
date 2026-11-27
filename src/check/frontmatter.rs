//! SKILL.md YAML 1.2 frontmatter parser and diagnostics.

use std::collections::{BTreeMap, HashMap};

use yaml_rust2::{
    parser::{Event, Parser, Tag},
    scanner::{Marker, TScalarStyle},
};

const MAX_NESTING_DEPTH: usize = 64;
const MAX_ALIAS_NODES: usize = 10_000;
const MAX_ALIAS_BYTES: usize = 1_048_576;

/// A frontmatter scalar or nested value, kept generic so validation can
/// report type errors.
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

/// Parse failure; validation reports it as a warning.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct FrontmatterError(pub String);

/// Split `text` into `(frontmatter map, body)`.
pub fn parse_frontmatter(
    text: &str,
) -> std::result::Result<(BTreeMap<String, Value>, String), FrontmatterError> {
    let (metadata, body) = parse_document(text)?;
    let metadata = metadata
        .into_iter()
        .map(|(key, value)| (key.to_text(), value.public_value()))
        .collect();
    Ok((metadata, body))
}

pub(crate) fn python_whitespace(c: char) -> bool {
    c.is_whitespace() || matches!(c, '\u{1c}'..='\u{1f}')
}

pub(crate) fn parse_document(text: &str) -> Result<(Mapping, String), FrontmatterError> {
    let (yaml, body) = split_document(text)?.ok_or_else(|| {
        FrontmatterError(
            if text.is_empty() {
                "file is empty: no frontmatter found"
            } else {
                "missing opening '---': frontmatter must start on the first line"
            }
            .into(),
        )
    })?;
    Ok((load_yaml(&yaml, None)?, body))
}

/// Return the YAML block and Markdown body independently of YAML parsing.
pub(crate) fn split_document(text: &str) -> Result<Option<(String, String)>, FrontmatterError> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let text = text.replace("\r\n", "\n");
    let mut lines = text.split_terminator(|c| {
        matches!(
            c,
            '\n' | '\r' | '\u{b}' | '\u{c}' | '\u{1c}'
                ..='\u{1e}' | '\u{85}' | '\u{2028}' | '\u{2029}'
        )
    });
    let delimiter = |line: &str| line.trim_matches(python_whitespace) == "---";
    if !lines.next().is_some_and(delimiter) {
        return Ok(None);
    }
    let mut yaml = Vec::new();
    for line in lines.by_ref() {
        if delimiter(line) {
            return Ok(Some((
                yaml.join("\n"),
                lines.collect::<Vec<_>>().join("\n"),
            )));
        }
        yaml.push(line);
    }
    Err(FrontmatterError(
        "unterminated frontmatter: no closing '---' delimiter found".into(),
    ))
}

pub(crate) type Mapping = Vec<(Node, Node)>;

pub(crate) fn mapping_get<'a>(metadata: &'a Mapping, name: &str) -> Option<&'a Node> {
    metadata
        .iter()
        .find_map(|(key, value)| matches!(key, Node::String(key) if key == name).then_some(value))
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Node {
    Null,
    Bool(bool),
    Int(String),
    Float(f64),
    String(String),
    List(Vec<Node>),
    Map(Mapping),
}

impl Node {
    pub(crate) fn truthy(&self) -> bool {
        match self {
            Self::Null => false,
            Self::Bool(value) => *value,
            Self::Int(value) => value
                .strip_prefix(['-', '+'])
                .unwrap_or(value)
                .bytes()
                .any(|b| b != b'0'),
            Self::Float(value) => *value != 0.0,
            Self::String(value) => !value.is_empty(),
            Self::List(items) => !items.is_empty(),
            Self::Map(entries) => !entries.is_empty(),
        }
    }

    fn type_name(&self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Bool(_) => "bool",
            Self::Int(_) => "int",
            Self::Float(_) => "float",
            Self::String(_) => "str",
            Self::List(_) => "list",
            Self::Map(_) => "map",
        }
    }

    pub(crate) fn to_text(&self) -> String {
        match self {
            Self::Null => "null".into(),
            Self::Bool(true) => "true".into(),
            Self::Bool(false) => "false".into(),
            Self::Int(value) | Self::String(value) => value.clone(),
            Self::Float(value) => value.to_string(),
            Self::List(items) => {
                let rendered: Vec<_> = items.iter().map(Self::to_text).collect();
                format!("[{}]", rendered.join(", "))
            }
            Self::Map(entries) => {
                let rendered: Vec<_> = entries
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k.to_text(), v.to_text()))
                    .collect();
                format!("{{{}}}", rendered.join(", "))
            }
        }
    }

    fn public_value(&self) -> Value {
        match self {
            Self::Null => Value::Null,
            Self::Bool(value) => Value::Bool(*value),
            Self::Int(value) => value
                .parse()
                .map(Value::Int)
                .unwrap_or_else(|_| Value::String(value.clone())),
            Self::Float(value) => Value::Float(*value),
            Self::String(value) => Value::String(value.clone()),
            Self::List(items) => Value::List(items.iter().map(Self::public_value).collect()),
            Self::Map(entries) => Value::Map(
                entries
                    .iter()
                    .map(|(key, value)| (key.to_text(), value.public_value()))
                    .collect(),
            ),
        }
    }
}

#[derive(Default)]
pub(crate) struct YamlDiagnostics {
    pub(crate) commented_fields: Vec<(String, String)>,
    pub(crate) plain_name: Option<String>,
    pub(crate) name_may_be_quoted: bool,
    pub(crate) description_may_be_quoted: bool,
}

#[derive(Clone, Default)]
struct ValueMeta {
    plain_text: Option<String>,
    inline_comment: bool,
    may_be_quoted: bool,
}

#[derive(Clone)]
struct Item {
    node: Node,
    meta: ValueMeta,
    depth: usize,
    nodes: usize,
    bytes: usize,
}

struct Frame {
    items: Vec<Item>,
    mapping: bool,
    anchor: usize,
    start: usize,
}

struct Loader<'a> {
    source: &'a str,
    track_diagnostics: bool,
    check_comments: bool,
    documents: usize,
    alias_nodes: usize,
    alias_bytes: usize,
    frames: Vec<Frame>,
    anchors: HashMap<usize, Item>,
    root: Option<Item>,
    root_metas: Vec<ValueMeta>,
}

impl<'a> Loader<'a> {
    fn new(source: &'a str, track_diagnostics: bool) -> Self {
        let check_comments = track_diagnostics && (source.contains(" #") || source.contains("\t#"));
        Self {
            source,
            track_diagnostics,
            check_comments,
            documents: 0,
            alias_nodes: 0,
            alias_bytes: 0,
            frames: Vec::new(),
            anchors: HashMap::new(),
            root: None,
            root_metas: Vec::new(),
        }
    }

    fn push_item(&mut self, item: Item, anchor: usize) {
        if anchor != 0 {
            self.anchors.insert(anchor, item.clone());
        }
        if let Some(frame) = self.frames.last_mut() {
            frame.items.push(item);
        } else {
            self.root = Some(item);
        }
    }

    fn handle_event(&mut self, event: Event, mark: Marker) -> Result<(), FrontmatterError> {
        match event {
            Event::DocumentStart => {
                self.documents += 1;
                if self.documents > 1 {
                    return Err(yaml_error("expected a single YAML document"));
                }
            }
            Event::Scalar(text, style, anchor, tag) => {
                let mut meta = ValueMeta::default();
                if self.track_diagnostics && style == TScalarStyle::Plain && !text.is_empty() {
                    if self.check_comments && has_inline_comment(self.source, mark.index(), &text) {
                        meta.inline_comment = true;
                    }
                    meta.plain_text = Some(text.clone());
                }
                let node = resolve_scalar(text, style, tag.as_ref())?;
                let bytes = match &node {
                    Node::Int(s) | Node::String(s) => s.len(),
                    _ => 0,
                }
                .saturating_add(meta.plain_text.as_ref().map_or(0, String::len));
                self.push_item(
                    Item {
                        node,
                        meta,
                        depth: 0,
                        nodes: 1,
                        bytes,
                    },
                    anchor,
                );
            }
            Event::MappingStart(anchor, ref tag) | Event::SequenceStart(anchor, ref tag) => {
                let mapping = matches!(event, Event::MappingStart(..));
                let expected = if mapping { "map" } else { "seq" };
                if let Some(tag) = tag.as_ref()
                    && !is_supported_collection_tag(tag, expected)
                {
                    return Err(unsupported_tag(tag));
                }
                if self.frames.len() >= MAX_NESTING_DEPTH {
                    return Err(yaml_error("frontmatter nesting limit exceeded"));
                }
                self.frames.push(Frame {
                    items: Vec::new(),
                    mapping,
                    anchor,
                    start: mark.index(),
                });
            }
            Event::MappingEnd | Event::SequenceEnd => {
                let Some(frame) = self.frames.pop() else {
                    return Ok(());
                };
                let anchor = frame.anchor;
                let may_be_quoted = self.track_diagnostics
                    && may_quote_collection(self.source, frame.start, mark.index());
                let (node, depth, nodes, bytes) = if frame.mapping {
                    let is_root = self.frames.is_empty();
                    let mut entries: Mapping = Vec::new();
                    let mut metas: Vec<ValueMeta> = Vec::new();
                    type Stat = (usize, usize, usize);
                    let mut stats: Vec<(Stat, Stat)> = Vec::new();
                    let mut items = frame.items.into_iter();
                    while let (Some(key), Some(value)) = (items.next(), items.next()) {
                        let key_stat = (key.depth, key.nodes, key.bytes);
                        let val_stat = (value.depth, value.nodes, value.bytes);
                        if let Some(pos) =
                            entries.iter().position(|(k, _)| keys_equal(k, &key.node))
                        {
                            entries[pos].1 = value.node;
                            stats[pos].1 = val_stat;
                            if is_root && self.track_diagnostics {
                                metas[pos] = value.meta;
                            }
                        } else {
                            if is_root && self.track_diagnostics {
                                metas.push(value.meta);
                            }
                            entries.push((key.node, value.node));
                            stats.push((key_stat, val_stat));
                        }
                    }
                    if is_root && self.track_diagnostics {
                        self.root_metas = metas;
                    }
                    let (max_child, total_nodes, total_bytes) = stats.into_iter().fold(
                        (0, 1usize, 0usize),
                        |(d, n, b), ((kd, kn, kb), (vd, vn, vb))| {
                            (
                                d.max(kd).max(vd),
                                n.saturating_add(kn).saturating_add(vn),
                                b.saturating_add(kb).saturating_add(vb),
                            )
                        },
                    );
                    (
                        Node::Map(entries),
                        max_child.saturating_add(1),
                        total_nodes,
                        total_bytes,
                    )
                } else {
                    let (mut max_child, mut total_nodes, mut total_bytes) =
                        (0usize, 1usize, 0usize);
                    let mut list = Vec::with_capacity(frame.items.len());
                    for item in frame.items {
                        max_child = max_child.max(item.depth);
                        total_nodes = total_nodes.saturating_add(item.nodes);
                        total_bytes = total_bytes.saturating_add(item.bytes);
                        list.push(item.node);
                    }
                    (
                        Node::List(list),
                        max_child.saturating_add(1),
                        total_nodes,
                        total_bytes,
                    )
                };
                let meta = ValueMeta {
                    may_be_quoted,
                    ..Default::default()
                };
                self.push_item(
                    Item {
                        node,
                        meta,
                        depth,
                        nodes,
                        bytes,
                    },
                    anchor,
                );
            }
            Event::Alias(id) => {
                let Some(aliased) = self.anchors.get(&id) else {
                    return Err(yaml_error("undefined or recursive alias"));
                };
                if self.frames.len().saturating_add(aliased.depth) > MAX_NESTING_DEPTH {
                    return Err(yaml_error("frontmatter nesting limit exceeded"));
                }
                self.alias_nodes = self.alias_nodes.saturating_add(aliased.nodes);
                self.alias_bytes = self.alias_bytes.saturating_add(aliased.bytes);
                if self.alias_nodes > MAX_ALIAS_NODES || self.alias_bytes > MAX_ALIAS_BYTES {
                    return Err(yaml_error("frontmatter size limit exceeded"));
                }
                let item = aliased.clone();
                self.push_item(item, 0);
            }
            Event::Nothing | Event::StreamStart | Event::StreamEnd | Event::DocumentEnd => {}
        }
        Ok(())
    }
}

pub(crate) fn load_yaml(
    source: &str,
    diagnostics: Option<&mut YamlDiagnostics>,
) -> Result<Mapping, FrontmatterError> {
    let mut parser = Parser::new_from_str(source);
    let mut loader = Loader::new(source, diagnostics.is_some());
    loop {
        let (event, mark) = parser.next_token().map_err(|err| {
            if err.info() == "recursion limit exceeded" {
                yaml_error("frontmatter nesting limit exceeded")
            } else {
                yaml_error(err)
            }
        })?;
        if matches!(event, Event::StreamEnd) {
            break;
        }
        loader.handle_event(event, mark)?;
    }
    match loader.root.map(|item| item.node) {
        None | Some(Node::Null) => Ok(Vec::new()),
        Some(Node::Map(mapping)) => {
            if let Some(diagnostics) = diagnostics {
                for ((key, node), meta) in mapping.iter().zip(&loader.root_metas) {
                    let Node::String(field) = key else {
                        continue;
                    };
                    let may_be_quoted = match field.as_str() {
                        "name" => &mut diagnostics.name_may_be_quoted,
                        "description" => &mut diagnostics.description_may_be_quoted,
                        _ => continue,
                    };
                    if let Some(plain) = &meta.plain_text {
                        if meta.inline_comment {
                            diagnostics
                                .commented_fields
                                .push((field.clone(), plain.clone()));
                        }
                        if field == "name" && !matches!(node, Node::Null | Node::String(_)) {
                            // Codex reads plain scalars into its string-valued name as written.
                            diagnostics.plain_name = Some(plain.clone());
                        }
                    }
                    *may_be_quoted = meta.may_be_quoted;
                }
            }
            Ok(mapping)
        }
        Some(node) => Err(FrontmatterError(format!(
            "frontmatter must be a YAML mapping, got {}",
            node.type_name()
        ))),
    }
}

fn canonical_int(text: &str) -> (bool, &str) {
    let neg = text.starts_with('-');
    let digits = text
        .strip_prefix(['-', '+'])
        .unwrap_or(text)
        .trim_start_matches('0');
    if digits.is_empty() {
        (false, "0")
    } else {
        (neg, digits)
    }
}

fn keys_equal(left: &Node, right: &Node) -> bool {
    match (left, right) {
        (Node::Int(a), Node::Int(b)) => canonical_int(a) == canonical_int(b),
        (Node::Float(a), Node::Float(b)) => a == b || (a.is_nan() && b.is_nan()),
        _ => left == right,
    }
}

fn resolve_scalar(
    text: String,
    style: TScalarStyle,
    tag: Option<&Tag>,
) -> Result<Node, FrontmatterError> {
    let tag = match tag {
        Some(tag) if non_specific_tag(tag) => return Ok(Node::String(text)),
        Some(tag) => match yaml_core_tag(tag) {
            Some(suffix) => Some(suffix),
            None => return Err(unsupported_tag(tag)),
        },
        None if style != TScalarStyle::Plain => return Ok(Node::String(text)),
        None => None,
    };
    match tag.as_deref() {
        Some("str") => Ok(Node::String(text)),
        Some("null") => is_null(&text)
            .then_some(Node::Null)
            .ok_or_else(|| yaml_error(format!("invalid !!null scalar: {text:?}"))),
        Some("bool") => parse_bool(&text)
            .map(Node::Bool)
            .ok_or_else(|| yaml_error(format!("invalid !!bool scalar: {text:?}"))),
        Some("int") => parse_int(&text)
            .map(Node::Int)
            .ok_or_else(|| yaml_error(format!("invalid !!int scalar: {text:?}"))),
        Some("float") => parse_float(&text, true)
            .map(Node::Float)
            .ok_or_else(|| yaml_error(format!("invalid !!float scalar: {text:?}"))),
        Some(other) => Err(yaml_error(format!(
            "unsupported tag: tag:yaml.org,2002:{other}"
        ))),
        None => Ok(if is_null(&text) {
            Node::Null
        } else if let Some(value) = parse_bool(&text) {
            Node::Bool(value)
        } else if let Some(value) = parse_int(&text) {
            Node::Int(value)
        } else if let Some(value) = parse_float(&text, false) {
            Node::Float(value)
        } else {
            Node::String(text)
        }),
    }
}

fn non_specific_tag(tag: &Tag) -> bool {
    tag.handle.is_empty() && tag.suffix == "!"
}

fn yaml_core_tag(tag: &Tag) -> Option<String> {
    format!("{}{}", tag.handle, tag.suffix)
        .strip_prefix("tag:yaml.org,2002:")
        .map(str::to_owned)
}

fn is_supported_collection_tag(tag: &Tag, expected: &str) -> bool {
    non_specific_tag(tag) || yaml_core_tag(tag).as_deref() == Some(expected)
}

fn is_null(text: &str) -> bool {
    matches!(text, "" | "~" | "null" | "Null" | "NULL")
}

fn parse_bool(text: &str) -> Option<bool> {
    match text {
        "true" | "True" | "TRUE" => Some(true),
        "false" | "False" | "FALSE" => Some(false),
        _ => None,
    }
}

fn parse_int(text: &str) -> Option<String> {
    let digits = text.strip_prefix(['-', '+']).unwrap_or(text);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(text.to_owned())
}

fn parse_float(text: &str, tagged: bool) -> Option<f64> {
    match text {
        ".nan" | ".NaN" | ".NAN" => return Some(f64::NAN),
        ".inf" | ".Inf" | ".INF" | "+.inf" | "+.Inf" | "+.INF" => return Some(f64::INFINITY),
        "-.inf" | "-.Inf" | "-.INF" => return Some(f64::NEG_INFINITY),
        _ => {}
    }
    let unsigned = text.strip_prefix(['-', '+']).unwrap_or(text);
    let (mantissa, exponent) = match unsigned.split_once(['e', 'E']) {
        Some((m, e)) => (m, Some(e)),
        None => (unsigned, None),
    };
    if let Some(exp) = exponent {
        let exp_digits = exp.strip_prefix(['-', '+']).unwrap_or(exp);
        if exp_digits.is_empty() || !exp_digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
    }
    if let Some((whole, frac)) = mantissa.split_once('.') {
        if (whole.is_empty() && frac.is_empty())
            || !whole.bytes().all(|b| b.is_ascii_digit())
            || !frac.bytes().all(|b| b.is_ascii_digit())
        {
            return None;
        }
    } else if (!tagged && exponent.is_none())
        || mantissa.is_empty()
        || !mantissa.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    text.parse().ok()
}

fn may_quote_collection(source: &str, start: usize, end: usize) -> bool {
    // Codex may quote an inline value containing a colon followed by space.
    // Use the parsed value's own span, including the origin of a YAML alias.
    let Some((start, '[' | '{')) = source.char_indices().nth(start) else {
        return false;
    };
    let Some((end, ']' | '}')) = source.char_indices().nth(end) else {
        return false;
    };
    let line_prefix = source[..start].rsplit('\n').next().unwrap_or_default();
    let Some((_, before_value)) = line_prefix.split_once(':') else {
        return false;
    };
    let value = &source[start..end];
    before_value.starts_with(char::is_whitespace)
        && !value.contains('\n')
        && value
            .match_indices(':')
            .any(|(index, _)| value[index + 1..].starts_with(char::is_whitespace))
}

fn has_inline_comment(source: &str, index: usize, value: &str) -> bool {
    let Some((start, _)) = source.char_indices().nth(index) else {
        return false;
    };
    let mut raw = source[start..].chars();
    // YAML folds whitespace across lines. Match the content to find the end
    // of the scalar, then look for a comment on that same physical line.
    let content = |c: &char| !matches!(c, ' ' | '\t' | '\n');
    for c in value.chars().filter(content) {
        if raw.find(content) != Some(c) {
            return false;
        }
    }
    let tail = raw.as_str();
    tail.starts_with([' ', '\t']) && tail.trim_start_matches([' ', '\t']).starts_with('#')
}

fn unsupported_tag(tag: &Tag) -> FrontmatterError {
    yaml_error(format!("unsupported tag: {}{}", tag.handle, tag.suffix))
}

fn yaml_error(details: impl std::fmt::Display) -> FrontmatterError {
    FrontmatterError(format!("malformed YAML in frontmatter: {details}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_distinct_key_types_in_internal_mapping() {
        let (mapping, _) = parse_document("---\n'1': text\n1: integer\n---").unwrap();
        assert_eq!(mapping.len(), 2);
        assert_eq!(
            mapping_get(&mapping, "1"),
            Some(&Node::String("text".into()))
        );
        assert_eq!(
            mapping[1],
            (Node::Int("1".into()), Node::String("integer".into()))
        );
    }
}
