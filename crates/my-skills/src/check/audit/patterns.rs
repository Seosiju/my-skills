use std::borrow::Cow;
use std::sync::LazyLock;

use regex::{Captures, Regex};

static PYTHON_WORD: LazyLock<Regex> = LazyLock::new(|| compile(r"[\p{L}\p{N}_]"));
static BOUNDARY_DIFFERENCES: LazyLock<Regex> =
    LazyLock::new(|| compile(r"[\w--[\p{L}\p{N}_]]|(?P<word>[\p{N}--\w])|(?P<i>[İı])"));

pub(super) fn compile(pattern: &str) -> Regex {
    Regex::new(pattern).expect("valid built-in audit regex")
}

pub(super) fn is_python_word(ch: char) -> bool {
    PYTHON_WORD.is_match(ch.encode_utf8(&mut [0; 4]))
}

// Only patterns that use \b and report booleans or line numbers use this
// text. Keep every newline and preserve membership in their other classes.
// Python's word characters are letters, numbers and _, without Rust's
// additional marks, connector punctuation and join controls. Python's
// IGNORECASE also treats the two non-ASCII forms of I as matching ASCII I.
pub(super) fn boundary_text(text: &str) -> Cow<'_, str> {
    BOUNDARY_DIFFERENCES.replace_all(text, |matched: &Captures<'_>| {
        if matched.name("word").is_some() {
            "界"
        } else if matched.name("i").is_some() {
            "i"
        } else {
            "\u{fffd}"
        }
    })
}

pub(super) fn line_number(text: &str, offset: usize) -> u32 {
    let newlines = text.as_bytes()[..offset]
        .iter()
        .filter(|&&byte| byte == b'\n')
        .count();
    u32::try_from(newlines + 1).unwrap_or(u32::MAX)
}
