//! SKILL.md YAML frontmatter. Stream D. Mirrors v0.4.1 `frontmatter.py`.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write;
use std::rc::Rc;

use yaml_rust2::{
    parser::{Event, Parser, Tag},
    scanner::{Marker, ScanError, Scanner, TScalarStyle, Token, TokenType},
};

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
/// Values outside the public representation use strings or lists.
pub fn parse_frontmatter(
    text: &str,
) -> std::result::Result<(BTreeMap<String, Value>, String), FrontmatterError> {
    let (metadata, body) = parse_document(text)?;
    let metadata = metadata
        .into_iter()
        .map(|(key, value)| (key.python_string(), value.public_value()))
        .collect();
    Ok((metadata, body))
}

pub(crate) fn python_whitespace(c: char) -> bool {
    c.is_whitespace() || matches!(c, '\u{1c}'..='\u{1f}')
}

pub(crate) fn parse_document(text: &str) -> Result<(Mapping, String), FrontmatterError> {
    parse_document_with_comments(text, None)
}

pub(crate) fn parse_document_with_comments(
    text: &str,
    commented_fields: Option<&mut Vec<(String, String)>>,
) -> Result<(Mapping, String), FrontmatterError> {
    if text.is_empty() {
        return Err(FrontmatterError(
            "file is empty: no frontmatter found".into(),
        ));
    }
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let text = text.replace("\r\n", "\n");
    // Python splitlines also recognizes Unicode line separators and a bare CR.
    let mut lines = text.split_terminator(|c| {
        matches!(
            c,
            '\n' | '\r' | '\u{b}' | '\u{c}' | '\u{1c}'
                ..='\u{1e}' | '\u{85}' | '\u{2028}' | '\u{2029}'
        )
    });
    let delimiter = |line: &str| line.trim_matches(python_whitespace) == "---";
    if !lines.next().is_some_and(delimiter) {
        return Err(FrontmatterError(
            "missing opening '---': frontmatter must start on the first line".into(),
        ));
    }
    let mut yaml = Vec::new();
    for line in lines.by_ref() {
        if delimiter(line) {
            return Ok((
                load_yaml(&yaml.join("\n"), commented_fields)?,
                lines.collect::<Vec<_>>().join("\n"),
            ));
        }
        yaml.push(line);
    }
    Err(FrontmatterError(
        "unterminated frontmatter: no closing '---' delimiter found".into(),
    ))
}

// Keep order and scalar types until validation has applied Python str() and
// truthiness. The public Value contract only supports string keys and i64.
pub(crate) type Mapping = Vec<(Rc<Node>, Rc<Node>)>;

#[derive(Debug, PartialEq, Clone)]
pub(crate) struct Located {
    node: Rc<Node>,
    index: usize,
}

#[derive(Debug, PartialEq)]
pub(crate) enum Node {
    Null,
    Bool(bool),
    Int(String),
    Float(f64),
    String(String),
    Bytes(Vec<u8>),
    Timestamp(Timestamp),
    List(Vec<Located>),
    Pair(Rc<Node>, Rc<Node>),
    Map(Mapping),
    Set(Vec<Rc<Node>>),
    MergeKey,
    ValueKey,
}

impl Node {
    pub(crate) fn truthy(&self) -> bool {
        match self {
            Self::Null => false,
            Self::Bool(value) => *value,
            Self::Int(value) => value != "0",
            Self::Float(value) => *value != 0.0,
            Self::String(value) => !value.is_empty(),
            Self::Bytes(value) => !value.is_empty(),
            Self::List(items) => !items.is_empty(),
            Self::Map(entries) => !entries.is_empty(),
            Self::Set(items) => !items.is_empty(),
            Self::Pair(..) | Self::Timestamp(_) | Self::MergeKey | Self::ValueKey => true,
        }
    }

    fn type_name(&self) -> &'static str {
        match self {
            Self::Null => "NoneType",
            Self::Bool(_) => "bool",
            Self::Int(_) => "int",
            Self::Float(_) => "float",
            Self::String(_) | Self::MergeKey | Self::ValueKey => "str",
            Self::Bytes(_) => "bytes",
            Self::Timestamp(time) if time.clock.is_some() => "datetime",
            Self::Timestamp(_) => "date",
            Self::List(_) => "list",
            Self::Pair(..) => "tuple",
            Self::Map(_) => "dict",
            Self::Set(_) => "set",
        }
    }

    pub(crate) fn python_string(&self) -> String {
        match self {
            Self::Null => "None".into(),
            Self::Bool(true) => "True".into(),
            Self::Bool(false) => "False".into(),
            Self::Int(value) | Self::String(value) => value.clone(),
            Self::Float(value) => python_float(*value),
            Self::Bytes(value) => python_bytes(value),
            Self::Timestamp(value) => value.python_string(),
            Self::List(items) => {
                let items = items
                    .iter()
                    .map(|item| item.node.python_repr())
                    .collect::<Vec<_>>();
                format!("[{}]", items.join(", "))
            }
            Self::Pair(key, value) => format!("({}, {})", key.python_repr(), value.python_repr()),
            Self::Map(entries) => {
                let entries = entries
                    .iter()
                    .map(|(key, value)| format!("{}: {}", key.python_repr(), value.python_repr()))
                    .collect::<Vec<_>>();
                format!("{{{}}}", entries.join(", "))
            }
            Self::Set(items) if items.is_empty() => "set()".into(),
            Self::Set(items) => {
                let items = items
                    .iter()
                    .map(|item| item.python_repr())
                    .collect::<Vec<_>>();
                format!("{{{}}}", items.join(", "))
            }
            Self::MergeKey => "<<".into(),
            Self::ValueKey => "=".into(),
        }
    }

    fn python_repr(&self) -> String {
        match self {
            Self::String(value) => python_quote(value),
            Self::Timestamp(value) => value.python_repr(),
            _ => self.python_string(),
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
            Self::List(items) => {
                Value::List(items.iter().map(|item| item.node.public_value()).collect())
            }
            Self::Pair(key, value) => Value::List(vec![key.public_value(), value.public_value()]),
            Self::Map(entries) => Value::Map(
                entries
                    .iter()
                    .map(|(key, value)| (key.python_string(), value.public_value()))
                    .collect(),
            ),
            Self::Set(items) => Value::List(items.iter().map(|item| item.public_value()).collect()),
            Self::String(_)
            | Self::Bytes(_)
            | Self::Timestamp(_)
            | Self::MergeKey
            | Self::ValueKey => Value::String(self.python_string()),
        }
    }
}

fn python_float(value: f64) -> String {
    if value.is_nan() {
        return "nan".into();
    }
    let rendered = format!("{value:?}");
    if let Some((mantissa, exponent)) = rendered.split_once('e') {
        let exponent = exponent.parse::<i32>().expect("f64 exponent fits in i32");
        format!("{mantissa}e{exponent:+03}")
    } else {
        rendered
    }
}

fn python_quote(value: &str) -> String {
    quote(value, false)
}

fn python_bytes(bytes: &[u8]) -> String {
    quote(
        &bytes.iter().copied().map(char::from).collect::<String>(),
        true,
    )
}

fn quote(value: &str, bytes: bool) -> String {
    let quote = if value.contains('\'') && !value.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut result = if bytes {
        String::from("b")
    } else {
        String::new()
    };
    result.push(quote);
    for c in value.chars() {
        match c {
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            '\\' => result.push_str("\\\\"),
            c if c == quote => {
                result.push('\\');
                result.push(c);
            }
            c if (bytes && !c.is_ascii())
                || c.is_control()
                || (c != ' ' && c.is_whitespace())
                || matches!(c, '\u{ad}' | '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2060}'..='\u{206f}' | '\u{feff}') =>
            {
                let code = u32::from(c);
                if code <= 0xff {
                    write!(result, "\\x{code:02x}").unwrap();
                } else {
                    write!(result, "\\u{code:04x}").unwrap();
                }
            }
            c => result.push(c),
        }
    }
    result.push(quote);
    result
}

#[derive(Debug, PartialEq)]
pub(crate) struct Timestamp {
    year: u16,
    month: u8,
    day: u8,
    clock: Option<Clock>,
}

#[derive(Debug, PartialEq)]
struct Clock {
    hour: u8,
    minute: u8,
    second: u8,
    micros: u32,
    offset: Option<i32>,
}

impl Timestamp {
    fn parse(text: &str) -> Option<Self> {
        let date_end = text.find(['T', 't', ' ', '\t']).unwrap_or(text.len());
        let mut date = text[..date_end].split('-');
        let year = digits(date.next()?, 4, 4)?;
        let min_width = if date_end == text.len() { 2 } else { 1 };
        let month = digits(date.next()?, min_width, 2)? as u8;
        let day = digits(date.next()?, min_width, 2)? as u8;
        if date.next().is_some() {
            return None;
        }
        let clock = match text.as_bytes().get(date_end) {
            None => None,
            Some(b'T' | b't') => Some(Clock::parse(&text[date_end + 1..])?),
            Some(_) => Some(Clock::parse(
                text[date_end..].trim_start_matches([' ', '\t']),
            )?),
        };
        Some(Self {
            year,
            month,
            day,
            clock,
        })
    }

    fn python_string(&self) -> String {
        let Self {
            year,
            month,
            day,
            clock,
        } = self;
        let mut value = format!("{year:04}-{month:02}-{day:02}");
        if let Some(Clock {
            hour,
            minute,
            second,
            micros,
            offset,
        }) = clock
        {
            write!(value, " {hour:02}:{minute:02}:{second:02}").unwrap();
            if *micros != 0 {
                write!(value, ".{micros:06}").unwrap();
            }
            if let Some(offset) = offset {
                let sign = if *offset < 0 { '-' } else { '+' };
                let offset = offset.abs();
                write!(value, "{sign}{:02}:{:02}", offset / 60, offset % 60).unwrap();
            }
        }
        value
    }

    fn python_repr(&self) -> String {
        let Self {
            year,
            month,
            day,
            clock,
        } = self;
        let Some(Clock {
            hour,
            minute,
            second,
            micros,
            offset,
        }) = clock
        else {
            return format!("datetime.date({year}, {month}, {day})");
        };
        let mut value = format!("datetime.datetime({year}, {month}, {day}, {hour}, {minute}");
        if *second != 0 || *micros != 0 {
            write!(value, ", {second}").unwrap();
        }
        if *micros != 0 {
            write!(value, ", {micros}").unwrap();
        }
        match *offset {
            None => {}
            Some(0) => value.push_str(", tzinfo=datetime.timezone.utc"),
            Some(minutes) => {
                let days = minutes.div_euclid(1440);
                let seconds = minutes.rem_euclid(1440) * 60;
                value.push_str(", tzinfo=datetime.timezone(datetime.timedelta(");
                if days != 0 {
                    write!(value, "days={days}, ").unwrap();
                }
                write!(value, "seconds={seconds}))").unwrap();
            }
        }
        value.push(')');
        value
    }
}

impl Clock {
    fn parse(text: &str) -> Option<Self> {
        let tz_start = text.find(['Z', '+', '-']).unwrap_or(text.len());
        let time = text[..tz_start].trim_end_matches([' ', '\t']);
        let offset = match &text[tz_start..] {
            "" => None,
            "Z" => Some(0),
            tz => {
                let mut parts = tz[1..].split(':');
                let hour = i32::from(digits(parts.next()?, 1, 2)?);
                let minute = match parts.next() {
                    Some(part) => i32::from(digits(part, 2, 2)?),
                    None => 0,
                };
                if parts.next().is_some() {
                    return None;
                }
                Some((hour * 60 + minute) * if tz.starts_with('-') { -1 } else { 1 })
            }
        };
        let mut parts = time.splitn(3, ':');
        let hour = digits(parts.next()?, 1, 2)? as u8;
        let minute = digits(parts.next()?, 2, 2)? as u8;
        let second = parts.next()?;
        let (second, fraction) = second.split_once('.').unwrap_or((second, ""));
        let second = digits(second, 2, 2)? as u8;
        if !fraction.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let micros = fraction
            .bytes()
            .chain(std::iter::repeat(b'0'))
            .take(6)
            .fold(0, |value, digit| value * 10 + u32::from(digit - b'0'));
        Some(Self {
            hour,
            minute,
            second,
            micros,
            offset,
        })
    }
}

fn digits(text: &str, min: usize, max: usize) -> Option<u16> {
    if (min..=max).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_digit()) {
        text.parse().ok()
    } else {
        None
    }
}

// The loader resolves YAML 1.2 scalars and rejects duplicates. Use the parser
// events for YAML syntax and apply PyYAML's SafeLoader rules to those events.
fn resolve_scalar(text: String, style: TScalarStyle, tag: Option<&Tag>) -> Option<Node> {
    let tag = match tag {
        Some(tag) if tag.handle == "tag:yaml.org,2002:" => Some(tag.suffix.as_str()),
        Some(tag) if non_specific_tag(tag) => None,
        Some(_) => return None,
        None if style != TScalarStyle::Plain => return Some(Node::String(text)),
        None => None,
    };
    let node = match tag {
        Some("str") => Node::String(text),
        Some("null") => Node::Null,
        Some("bool") => Node::Bool(parse_bool(&text.to_ascii_lowercase())?),
        Some("int") => Node::Int(parse_int(&text, true)?),
        Some("float") => Node::Float(parse_float(&text, true)?),
        Some("binary") => Node::Bytes(decode_bytes(&text)?),
        Some("timestamp") => Node::Timestamp(Timestamp::parse(&text)?),
        Some("merge") => Node::MergeKey,
        Some("value") => Node::ValueKey,
        Some(_) => return None,
        None => {
            if matches!(text.as_str(), "" | "~" | "null" | "Null" | "NULL") {
                Node::Null
            } else if let Some(value) = parse_bool(&text) {
                Node::Bool(value)
            } else if let Some(value) = parse_int(&text, false) {
                Node::Int(value)
            } else if let Some(value) = parse_float(&text, false) {
                Node::Float(value)
            } else if let Some(value) = Timestamp::parse(&text) {
                Node::Timestamp(value)
            } else if text == "<<" {
                Node::MergeKey
            } else if text == "=" {
                Node::ValueKey
            } else {
                Node::String(text)
            }
        }
    };
    Some(node)
}

fn decode_bytes(text: &str) -> Option<Vec<u8>> {
    if !text.is_ascii() {
        return None;
    }
    let mut bytes = Vec::new();
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut count = 0;
    let mut bits = 0;
    let mut padding = 0;
    for byte in text.bytes() {
        if byte == b'=' && count >= 2 {
            padding += 1;
            if count + padding == 4 {
                return Some(bytes);
            }
        } else if let Some(digit) = alphabet.iter().position(|&c| c == byte) {
            padding = 0;
            bits = (bits << 6) | digit as u16;
            count = (count + 1) % 4;
            if count != 1 {
                let remaining = (6 * count) % 8;
                bytes.push((bits >> remaining) as u8);
            }
        }
    }
    (count == 0).then_some(bytes)
}

fn non_specific_tag(tag: &Tag) -> bool {
    tag.handle.is_empty() && tag.suffix == "!"
}

fn parse_bool(text: &str) -> Option<bool> {
    match text {
        "yes" | "Yes" | "YES" | "true" | "True" | "TRUE" | "on" | "On" | "ON" => Some(true),
        "no" | "No" | "NO" | "false" | "False" | "FALSE" | "off" | "Off" | "OFF" => Some(false),
        _ => None,
    }
}

fn unsigned(text: &str) -> &str {
    text.strip_prefix(['-', '+']).unwrap_or(text)
}

fn radix_digits(text: &str, radix: u32) -> bool {
    !text.is_empty() && text.chars().all(|c| c == '_' || c.is_digit(radix))
}

fn sexagesimal_digit(text: &str) -> Option<u16> {
    digits(text, 1, 2).filter(|digit| *digit < 60)
}

fn parse_int(text: &str, tagged: bool) -> Option<String> {
    let value = unsigned(text);
    let (base, value) = if let Some(rest) = value.strip_prefix("0b") {
        (2, rest)
    } else if let Some(rest) = value.strip_prefix("0x") {
        (16, rest)
    } else if value != "0" && value.starts_with('0') {
        (8, value)
    } else {
        (10, value)
    };

    let mut parts = value.split(':');
    let first = parts.next()?;
    if !radix_digits(first, base)
        || (!tagged && first.starts_with('_'))
        || (value.contains(':') && (base != 10 || first == "0"))
    {
        return None;
    }

    if base == 10 && !value.contains(':') {
        let decimal = first.replace('_', "");
        return Some(if text.starts_with('-') && decimal != "0" {
            format!("-{decimal}")
        } else {
            decimal
        });
    }

    // Python's integers can exceed i64. Accumulate decimal digits so str()
    // remains exact for arbitrarily large base-prefixed integers as well.
    let mut decimal = vec![0u32];
    let mut multiply_add = |multiplier: u32, mut carry: u32| {
        for digit in &mut decimal {
            carry += *digit * multiplier;
            *digit = carry % 10;
            carry /= 10;
        }
        while carry != 0 {
            decimal.push(carry % 10);
            carry /= 10;
        }
    };
    for digit in first.chars().filter(|&c| c != '_') {
        multiply_add(base, digit.to_digit(base)?);
    }
    for part in parts {
        multiply_add(60, u32::from(sexagesimal_digit(part)?));
    }
    let sign = if text.starts_with('-') && decimal != [0] {
        "-"
    } else {
        ""
    };
    Some(
        sign.chars()
            .chain(
                decimal
                    .into_iter()
                    .rev()
                    .map(|digit| char::from(b'0' + digit as u8)),
            )
            .collect(),
    )
}

fn parse_float(text: &str, tagged: bool) -> Option<f64> {
    match text {
        ".nan" | ".NaN" | ".NAN" => return Some(f64::NAN),
        _ => {}
    }
    let value = unsigned(text);
    if tagged && value.eq_ignore_ascii_case(".nan") {
        return Some(f64::NAN);
    }
    if matches!(value, ".inf" | ".Inf" | ".INF") {
        return Some(if text.starts_with('-') {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        });
    }
    let (mantissa, exponent) = value.split_once(['e', 'E']).unwrap_or((value, ""));
    if !tagged
        && value.contains(['e', 'E'])
        && !(exponent.starts_with(['-', '+'])
            && !unsigned(exponent).is_empty()
            && unsigned(exponent).bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    let (whole, fraction) = match mantissa.split_once('.') {
        Some(parts) => parts,
        None if tagged => return text.replace('_', "").parse().ok(),
        None => return None,
    };
    if let Some((first, rest)) = whole.split_once(':') {
        if !first.starts_with(|c: char| c.is_ascii_digit())
            || !radix_digits(first, 10)
            || !fraction.bytes().all(|b| b.is_ascii_digit() || b == b'_')
            || value.contains(['e', 'E'])
        {
            return None;
        }
        let mut total = first.replace('_', "").parse::<f64>().ok()?;
        for segment in rest.split(':') {
            total = total * 60.0 + f64::from(sexagesimal_digit(segment)?);
        }
        let fraction = format!("0.{fraction}")
            .replace('_', "")
            .parse::<f64>()
            .ok()?;
        return Some((total + fraction) * if text.starts_with('-') { -1.0 } else { 1.0 });
    }
    if !tagged
        && ((!whole.is_empty() && !whole.starts_with(|c: char| c.is_ascii_digit()))
            || (whole.is_empty()
                && (text != value || !fraction.starts_with(|c: char| c.is_ascii_digit()))))
    {
        return None;
    }
    text.replace('_', "").parse().ok()
}

struct Frame {
    items: Vec<Located>,
    mapping: bool,
    anchor: usize,
    start: usize,
    tag: Option<String>,
}

impl Frame {
    fn finish(self, source: &str) -> Result<Node, FrontmatterError> {
        if !self.mapping {
            if let Some(tag) = self
                .tag
                .as_deref()
                .filter(|tag| matches!(*tag, "omap" | "pairs"))
            {
                let mut items = self.items;
                let kind = if tag == "pairs" {
                    "pairs"
                } else {
                    "an ordered map"
                };
                for item in &mut items {
                    let pair = match item.node.as_ref() {
                        Node::Map(pairs) => {
                            if let [(key, value)] = pairs.as_slice() {
                                Ok(Node::Pair(Rc::clone(key), Rc::clone(value)))
                            } else {
                                Err(format!(
                                    "expected a single mapping item, but found {} items",
                                    pairs.len()
                                ))
                            }
                        }
                        Node::List(_) => {
                            Err("expected a mapping of length 1, but found sequence".into())
                        }
                        _ => Err("expected a mapping of length 1, but found scalar".into()),
                    }
                    .map_err(|problem| {
                        yaml_error(format!(
                            "while constructing {kind}\n{}\n{problem}\n{}",
                            location(source, self.start),
                            location(source, item.index)
                        ))
                    })?;
                    item.node = Rc::new(pair);
                }
                return Ok(Node::List(items));
            }
            return Ok(Node::List(self.items));
        }
        let mut merged = Vec::new();
        let mut entries = Vec::new();
        let mut items = self.items.into_iter();
        while let (Some(mut key), Some(value)) = (items.next(), items.next()) {
            if matches!(*key.node, Node::MergeKey) {
                let mut merge_mapping = |value: &Located, expected: &str| {
                    if let Node::Map(pairs) = value.node.as_ref() {
                        merged.extend(pairs.iter().cloned());
                        Ok(())
                    } else {
                        let kind = match *value.node {
                            Node::List(_) => "sequence",
                            _ => "scalar",
                        };
                        Err(yaml_error(format!(
                            "while constructing a mapping\n{}\nexpected {expected} for merging, but found {kind}\n{}",
                            location(source, self.start),
                            location(source, value.index)
                        )))
                    }
                };
                if let Node::List(items) = value.node.as_ref() {
                    // Earlier maps in a YAML merge sequence have higher precedence.
                    for item in items {
                        if !matches!(*item.node, Node::Map(_)) {
                            merge_mapping(item, "a mapping")?;
                        }
                    }
                    for item in items.iter().rev() {
                        merge_mapping(item, "a mapping")?;
                    }
                } else {
                    merge_mapping(&value, "a mapping or list of mappings")?;
                }
            } else if matches!(*key.node, Node::List(_) | Node::Map(_) | Node::Set(_)) {
                return Err(yaml_error(format!(
                    "while constructing a mapping\n{}\nfound unhashable key\n{}",
                    location(source, self.start),
                    location(source, key.index)
                )));
            } else {
                if matches!(*key.node, Node::ValueKey) {
                    key.node = Rc::new(Node::String("=".into()));
                }
                entries.push((key.node, value.node));
            }
        }

        let mut mapping: Mapping = Vec::new();
        for (key, value) in merged.into_iter().chain(entries) {
            if let Some((_, existing)) = mapping
                .iter_mut()
                .find(|(existing, _)| keys_equal(existing, &key))
            {
                *existing = value;
            } else {
                mapping.push((key, value));
            }
        }
        if self.tag.as_deref() == Some("set") {
            Ok(Node::Set(mapping.into_iter().map(|(key, _)| key).collect()))
        } else {
            Ok(Node::Map(mapping))
        }
    }
}

fn keys_equal(left: &Rc<Node>, right: &Rc<Node>) -> bool {
    if Rc::ptr_eq(left, right) || left == right {
        return true;
    }
    // In Python, booleans are integers and an integral float can be the
    // same key as an integer. Preserve the first key and the last value.
    let compare = |key: &Node, other: &Node| match (key, other) {
        (Node::Int(value), Node::Bool(other)) => value == if *other { "1" } else { "0" },
        (Node::Float(value), Node::Bool(other)) => *value == f64::from(*other),
        (Node::Float(value), Node::Int(other)) => {
            value.is_finite() && value.fract() == 0.0 && format!("{:.0}", value + 0.0) == *other
        }
        (Node::Float(left), Node::Float(right)) => left.is_nan() && right.is_nan(),
        _ => false,
    };
    compare(left, right) || compare(right, left)
}

fn load_yaml(
    source: &str,
    commented_fields: Option<&mut Vec<(String, String)>>,
) -> Result<Mapping, FrontmatterError> {
    let mut parser = Parser::new_from_str(source);
    let mut frames: Vec<Frame> = Vec::new();
    let mut anchors = HashMap::new();
    let check_comments =
        commented_fields.is_some() && (source.contains(" #") || source.contains("\t#"));
    let mut plain_scalars = Vec::new();
    let mut root = None;
    let mut document_start = None;
    if let Some((index, c)) = source.chars().enumerate().find(|&(_, c)| {
        !matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{7e}' | '\u{85}' | '\u{a0}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}')
    }) {
        return Err(yaml_error(format!(
            "unacceptable character #x{:04x}: special characters are not allowed\n  in \"<unicode string>\", position {index}", u32::from(c)
        )));
    }
    if source.contains('&') {
        check_duplicate_anchors(source)?;
    }
    loop {
        let (event, mark) = parser
            .next_token()
            .map_err(|error| syntax_error(source, &frames, &error))?;
        let mut index = mark.index();
        let (node, anchor) = match event {
            Event::StreamEnd => break,
            Event::DocumentStart => {
                if let Some(start) = document_start {
                    let token = token_name(source, index);
                    if token != "<document start>" {
                        return Err(yaml_error(format!(
                            "expected '<document start>', but found '{token}'\n{}",
                            location(source, collection_start(source, index))
                        )));
                    }
                    return Err(yaml_error(format!(
                        "expected a single document in the stream\n{}\nbut found another document\n{}",
                        location(source, start),
                        location(source, index)
                    )));
                }
                document_start = Some(collection_start(source, index));
                continue;
            }
            Event::Scalar(text, style, anchor, tag) => {
                let plain = (check_comments && style == TScalarStyle::Plain && !text.is_empty())
                    .then(|| text.clone());
                let node = Rc::new(
                    resolve_scalar(text, style, tag.as_ref())
                        .ok_or_else(|| unsupported_tag(source, mark, tag.as_ref()))?,
                );
                if let Some(value) = plain {
                    plain_scalars.push((Rc::clone(&node), index, value));
                }
                (node, anchor)
            }
            Event::MappingStart(..) | Event::SequenceStart(..) => {
                let (anchor, tag, kind) = match event {
                    Event::MappingStart(anchor, tag) => (anchor, tag, "map"),
                    Event::SequenceStart(anchor, tag) => (anchor, tag, "seq"),
                    _ => unreachable!(),
                };
                if let Some(tag) = tag.as_ref() {
                    if !(non_specific_tag(tag)
                        || (tag.handle == "tag:yaml.org,2002:"
                            && (tag.suffix == kind
                                || matches!(
                                    (kind, tag.suffix.as_str()),
                                    ("map", "set") | ("seq", "omap" | "pairs")
                                ))))
                    {
                        return Err(unsupported_tag(source, mark, Some(tag)));
                    }
                }
                if frames.len() == 128 {
                    return Err(yaml_error("frontmatter nesting limit exceeded".into()));
                }
                let start = collection_start(source, index);
                frames.push(Frame {
                    items: Vec::new(),
                    mapping: kind == "map",
                    anchor,
                    start,
                    tag: tag.map(|tag| tag.suffix),
                });
                continue;
            }
            Event::MappingEnd | Event::SequenceEnd => {
                let frame = frames.pop().expect("end event has an open collection");
                let anchor = frame.anchor;
                index = frame.start;
                (Rc::new(frame.finish(source)?), anchor)
            }
            Event::Alias(id) => {
                let value: &Located = anchors.get(&id).ok_or_else(|| {
                    yaml_error(format!(
                        "recursive alias cannot be represented\n{}",
                        location(source, mark.index())
                    ))
                })?;
                index = value.index;
                (Rc::clone(&value.node), 0)
            }
            _ => continue,
        };
        let special_tag = match *node {
            Node::MergeKey => Some("merge"),
            Node::ValueKey => Some("value"),
            _ => None,
        };
        if let Some(tag) = special_tag {
            if !frames
                .last()
                .is_some_and(|frame| frame.mapping && frame.items.len() % 2 == 0)
            {
                return Err(yaml_error(format!(
                    "could not determine a constructor for the tag 'tag:yaml.org,2002:{tag}'\n{}",
                    location(source, index),
                )));
            }
        }
        let item = Located { node, index };
        if anchor != 0 {
            anchors.insert(anchor, item.clone());
        }
        if let Some(frame) = frames.last_mut() {
            frame.items.push(item);
        } else {
            root = Some(item);
        }
    }
    match root.as_ref().map(|item| item.node.as_ref()) {
        None | Some(Node::Null) => Ok(Vec::new()),
        Some(Node::Map(mapping)) => {
            if let Some(commented_fields) = commented_fields {
                for (key, effective_value) in mapping {
                    // Follow the effective scalar through aliases, merges, and overrides.
                    if let Node::String(field) = key.as_ref()
                        && matches!(field.as_str(), "name" | "description")
                        && let Some((_, index, value)) = plain_scalars
                            .iter()
                            .find(|(node, _, _)| Rc::ptr_eq(node, effective_value))
                        && has_inline_comment(source, *index, value)
                    {
                        commented_fields.push((field.clone(), value.clone()));
                    }
                }
            }
            Ok(mapping.clone())
        }
        Some(node) => Err(FrontmatterError(format!(
            "frontmatter must be a YAML mapping, got {}",
            node.type_name()
        ))),
    }
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

fn yaml_error(details: String) -> FrontmatterError {
    FrontmatterError(format!("malformed YAML in frontmatter: {details}"))
}

fn check_duplicate_anchors(source: &str) -> Result<(), FrontmatterError> {
    let mut anchors = HashMap::new();
    for Token(mark, token) in Scanner::new(source.chars()) {
        match token {
            TokenType::DocumentStart => anchors.clear(),
            TokenType::Anchor(name) => {
                if let Some(first) = anchors.insert(name.clone(), mark.index()) {
                    return Err(yaml_error(format!(
                        "found duplicate anchor {}; first occurrence\n{}\nsecond occurrence\n{}",
                        python_quote(&name),
                        location(source, first),
                        location(source, mark.index())
                    )));
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn unsupported_tag(source: &str, mark: Marker, tag: Option<&Tag>) -> FrontmatterError {
    let Some(tag) = tag else {
        return yaml_error(format!(
            "invalid YAML scalar\n{}",
            location(source, mark.index())
        ));
    };
    let tag_name = format!("{}{}", tag.handle, tag.suffix);
    // The parser marks the scalar or collection rather than its preceding tag.
    let tag_index = Scanner::new(source.chars())
        .take_while(|Token(token_mark, _)| token_mark.index() <= mark.index())
        .filter_map(|Token(token_mark, token)| {
            matches!(token, TokenType::Tag(..)).then_some(token_mark.index())
        })
        .last()
        .unwrap_or(mark.index());
    yaml_error(format!(
        "could not determine a constructor for the tag {}\n{}",
        python_quote(&tag_name),
        location(source, tag_index)
    ))
}

fn location(source: &str, index: usize) -> String {
    let index = index.min(source.chars().count());
    let mut start = 0;
    for (line_index, line) in source.split('\n').enumerate() {
        let length = line.chars().count();
        if index <= start + length {
            let column = index - start;
            // Match the default PyYAML snippet limits around its caret.
            let mut left = 0;
            let mut right = length;
            let mut head = "";
            let mut tail = "";
            if column > 36 {
                left = column - 32;
                head = " ... ";
            }
            if length - column > 36 {
                right = column + 32;
                tail = " ... ";
            }
            let snippet: String = line.chars().skip(left).take(right - left).collect();
            let caret = " ".repeat(4 + head.len() + column - left);
            return format!(
                "  in \"<unicode string>\", line {}, column {}:\n    {head}{snippet}{tail}\n{caret}^",
                line_index + 1,
                column + 1
            );
        }
        start += length + 1;
    }
    unreachable!("index clamped to the source length")
}

fn quoted_scalar_closes(source: &str, index: usize) -> bool {
    if let Some(quote) = source.chars().nth(index) {
        let mut chars = source.chars().skip(index + 1).peekable();
        while let Some(c) = chars.next() {
            if (quote == '"' && c == '\\')
                || (quote == '\'' && c == quote && chars.peek() == Some(&quote))
            {
                chars.next();
            } else if c == quote {
                return true;
            }
        }
    }
    false
}

fn token_at(source: &str, index: usize) -> Option<TokenType> {
    Scanner::new(source.chars())
        .find(|token| token.0.index() == index)
        .map(|token| token.1)
}

fn token_name(source: &str, index: usize) -> &str {
    match token_at(source, index) {
        Some(TokenType::BlockMappingStart) => "<block mapping start>",
        Some(TokenType::BlockSequenceStart) => "<block sequence start>",
        Some(TokenType::BlockEnd) => "<block end>",
        Some(TokenType::Key) => "?",
        Some(TokenType::Value) => ":",
        Some(TokenType::BlockEntry) => "-",
        Some(TokenType::Scalar(..)) => "<scalar>",
        Some(TokenType::Anchor(_)) => "<anchor>",
        Some(TokenType::Alias(_)) => "<alias>",
        Some(TokenType::Tag(..)) => "<tag>",
        Some(TokenType::DocumentStart) => "<document start>",
        Some(TokenType::DocumentEnd) => "<document end>",
        _ => match source.chars().nth(index) {
            Some('[') => "[",
            Some(']') => "]",
            Some('{') => "{",
            Some('}') => "}",
            Some(',') => ",",
            None => "<stream end>",
            _ => "<scalar>",
        },
    }
}

fn collection_start(source: &str, index: usize) -> usize {
    // yaml-rust2 can mark a block mapping at its first colon. PyYAML uses
    // the beginning of the key, including for the mapping in a list item.
    if source.chars().nth(index) != Some(':') {
        return index;
    }
    let prefix: String = source.chars().take(index).collect();
    let line = prefix.rsplit('\n').next().unwrap_or_default();
    let mut key = line.trim_start_matches(' ');
    while let Some(rest) = key.strip_prefix("- ") {
        key = rest.trim_start_matches(' ');
    }
    index - key.chars().count()
}

fn syntax_error(source: &str, frames: &[Frame], error: &ScanError) -> FrontmatterError {
    let length = source.chars().count();
    let index = error.marker().index().min(length);
    let mark = |index| location(source, index);
    let with_context = |context, start, problem: &str, end| {
        if start == end {
            format!("{context}\n{problem}\n{}", mark(end))
        } else {
            format!("{context}\n{}\n{problem}\n{}", mark(start), mark(end))
        }
    };

    let details = match error.info() {
        "while scanning a quoted scalar, found unexpected end of stream"
        | "invalid indentation in quoted scalar"
            if !quoted_scalar_closes(source, index) =>
        {
            with_context(
                "while scanning a quoted scalar",
                index,
                "found unexpected end of stream",
                length,
            )
        }
        "while parsing node, found unknown anchor" => {
            let alias: String = source
                .chars()
                .skip(index + 1)
                .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
                .collect();
            format!(
                "found undefined alias {}\n{}",
                python_quote(&alias),
                mark(index)
            )
        }
        "mapping values are not allowed in this context" => {
            format!("mapping values are not allowed here\n{}", mark(index))
        }
        "simple key expect ':'" | "simple key expected" => {
            let line: String = source.chars().take(index).collect();
            let line = line.trim_end_matches('\n');
            let last_line = line.rsplit('\n').next().unwrap_or_default();
            let start = line.chars().count() - last_line.chars().count()
                + last_line.chars().take_while(|c| *c == ' ').count();
            with_context(
                "while scanning a simple key",
                start,
                "could not find expected ':'",
                index,
            )
        }
        info if info.contains("flow sequence, expected")
            || info.contains("flow mapping, did not find")
            || info == "invalid indentation in flow construct" =>
        {
            let flow =
                frames
                    .iter()
                    .rev()
                    .find_map(|frame| match source.chars().nth(frame.start) {
                        Some('[') => Some((frame.start, "sequence", ']')),
                        Some('{') => Some((frame.start, "mapping", '}')),
                        _ => None,
                    });
            let Some((start, kind, closing)) = flow else {
                return yaml_error(error.to_string());
            };
            let index = if info == "invalid indentation in flow construct" {
                length
            } else {
                index
            };
            with_context(
                &format!("while parsing a flow {kind}"),
                start,
                &format!(
                    "expected ',' or '{closing}', but got '{}'",
                    token_name(source, index)
                ),
                index,
            )
        }
        "while parsing a block mapping, did not find expected key"
        | "while parsing a block collection, did not find expected '-' indicator"
        | "invalid trailing content after double-quoted scalar"
            if !frames.is_empty() =>
        {
            let context = if frames.last().is_some_and(|frame| frame.mapping) {
                "while parsing a block mapping"
            } else {
                "while parsing a block collection"
            };
            let start = frames.last().map_or(index, |frame| frame.start);
            with_context(
                context,
                start,
                &format!(
                    "expected <block end>, but found '{}'",
                    token_name(source, index)
                ),
                collection_start(source, index),
            )
        }
        "did not find expected <document start>"
        | "missing explicit document end marker before directive" => format!(
            "expected '<document start>', but found '{}'\n{}",
            token_name(source, index),
            mark(index)
        ),
        "while parsing a node, did not find expected node content" => {
            let context = if frames
                .iter()
                .any(|frame| matches!(source.chars().nth(frame.start), Some('[' | '{')))
            {
                "while parsing a flow node"
            } else {
                "while parsing a block node"
            };
            with_context(
                context,
                index,
                &format!(
                    "expected the node content, but found '{}'",
                    token_name(source, index)
                ),
                index,
            )
        }
        info if info.starts_with("unexpected character:")
            || info.contains("tabs disallowed")
            || info.contains("tab cannot be used")
            || info.contains("invalid tab")
            || info.contains("found a tab")
            || info == "':' must be followed by a valid YAML whitespace" =>
        {
            let index = if info.contains("found a tab") || info.starts_with("':'") {
                source
                    .chars()
                    .enumerate()
                    .skip(index)
                    .find(|&(_, c)| c == '\t')
                    .map_or(index, |(pos, _)| pos)
            } else if info.contains("tab") {
                let prefix: String = source.chars().take(index + 1).collect();
                prefix
                    .rfind('\t')
                    .map_or(index, |byte| prefix[..byte].chars().count())
            } else {
                index
            };
            let c: String = source.chars().skip(index).take(1).collect();
            format!(
                "while scanning for the next token\nfound character {} that cannot start any token\n{}",
                python_quote(&c),
                mark(index)
            )
        }
        "while scanning a block scalar, did not find expected comment or line break"
        | "while scanning a block scalar, found an indentation indicator equal to 0" => {
            let mut chars = source.chars().enumerate().skip(index + 1);
            let problem = if let Some((position, c)) = chars.next() {
                if c == '0'
                    || matches!(c, '+' | '-') && chars.clone().next().is_some_and(|(_, c)| c == '0')
                {
                    (
                        "expected indentation indicator in the range 1-9, but found 0".into(),
                        if c == '0' { position } else { position + 1 },
                    )
                } else if c.is_ascii_digit() || matches!(c, '+' | '-' | ' ') {
                    let (position, c) = chars
                        .find(|&(_, c)| !matches!(c, '+' | '-' | '0'..='9' | ' '))
                        .unwrap_or((length, '\0'));
                    (
                        format!(
                            "expected a comment or a line break, but found {}",
                            python_quote(&c.to_string())
                        ),
                        position,
                    )
                } else {
                    (
                        format!(
                            "expected chomping or indentation indicators, but found {}",
                            python_quote(&c.to_string())
                        ),
                        position,
                    )
                }
            } else {
                return yaml_error(error.to_string());
            };
            with_context(
                "while scanning a block scalar",
                index,
                &problem.0,
                problem.1,
            )
        }
        "while parsing a quoted scalar, found unknown escape character"
        | "while parsing a quoted scalar, did not find expected hexadecimal number" => {
            let mut chars = source.chars().enumerate().skip(index + 1);
            let mut problem = None;
            while let Some((_, c)) = chars.next() {
                if c == '\\' {
                    if let Some((position, escaped)) = chars.next() {
                        if let Some(width) = match escaped {
                            'x' => Some(2),
                            'u' => Some(4),
                            'U' => Some(8),
                            _ => None,
                        } {
                            let mut digits = chars.by_ref().take(width);
                            if let Some((_, c)) = digits.find(|(_, c)| !c.is_ascii_hexdigit()) {
                                problem = Some((
                                    position + 1,
                                    format!(
                                        "expected escape sequence of {width} hexadecimal numbers, but found {}",
                                        python_quote(&c.to_string())
                                    ),
                                ));
                                break;
                            }
                        } else if !"0abtnvfre \"/\\N_LP\t\n".contains(escaped) {
                            problem = Some((
                                position,
                                format!(
                                    "found unknown escape character {}",
                                    python_quote(&escaped.to_string())
                                ),
                            ));
                            break;
                        }
                    }
                }
            }
            let Some((position, problem)) = problem else {
                return yaml_error(error.to_string());
            };
            with_context(
                "while scanning a double-quoted scalar",
                index,
                &problem,
                position,
            )
        }
        _ => return yaml_error(error.to_string()),
    };
    yaml_error(details)
}
