use std::sync::LazyLock;

use regex::Regex;

use super::analyzers::FileContext;
use super::patterns::{boundary_text, compile};
use super::{AuditFinding, rules::finding};

static PRIVATE_KEY: LazyLock<Regex> =
    LazyLock::new(|| compile(r"-----BEGIN [A-Z0-9 ]*PRIVATE KEY-----"));
static AWS_KEY: LazyLock<Regex> = LazyLock::new(|| compile(r"\bAKIA[0-9A-Z]{16}\b"));
static SECRET_ASSIGN: LazyLock<Regex> = LazyLock::new(|| {
    compile(
        r#"(?i)\b(?:api[_-]?key|secret|access[_-]?token|auth[_-]?token|password|passwd)\b[\s\x1c-\x1f]*[:=][\s\x1c-\x1f]*['"]?[A-Za-z0-9_\-/+]{8,}"#,
    )
});
static ABS_USER_PATH: LazyLock<Regex> =
    LazyLock::new(|| compile(r"/Users/[^/\s\x1c-\x1f]+|/home/[^/\s\x1c-\x1f]+"));
static PROMPT_INJECTION: LazyLock<Regex> = LazyLock::new(|| {
    compile(
        r"(?i)\b(ignore|override|forget)[\s\x1c-\x1f]+(all[\s\x1c-\x1f]+)?(previous|prior|system)[\s\x1c-\x1f]+instructions\b",
    )
});
static OUTPUT_SUPPRESSION: LazyLock<Regex> = LazyLock::new(|| {
    compile(
        r"(?i)\b(do not|don't)[\s\x1c-\x1f]+(tell|mention|report|show)[\s\x1c-\x1f]+(the[\s\x1c-\x1f]+)?user\b",
    )
});
static DESTRUCTIVE: LazyLock<Regex> =
    LazyLock::new(|| compile(r"\brm[\s\x1c-\x1f]+-rf[\s\x1c-\x1f]+(?:/|\$HOME|~|[.]{1,2}/)"));
static FETCH_SECRET: LazyLock<Regex> =
    LazyLock::new(|| compile(r"(?i)\b(?:curl|wget)\b.*\b(?:token|secret|password|api[_-]?key)\b"));

pub(super) fn analyze(context: &FileContext, findings: &mut Vec<AuditFinding>) {
    let mut report = |id, message| findings.push(finding(id, &context.file, message, None));
    if context.has_unexpected_nul {
        report("nul-byte", "file contains a NUL byte");
    }
    if context.decode_error {
        report("encoding", "file is not valid UTF-8");
        return;
    }
    let Some(text) = context.text.as_deref() else {
        return;
    };
    if text.chars().any(|ch| {
        matches!(ch, '\u{200e}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
    }) {
        report("bidi-unicode", "hidden/bidirectional Unicode control character");
    }

    let with_boundaries = boundary_text(text);
    let checks = [
        (
            &*PRIVATE_KEY,
            text,
            "private-key",
            "embedded private key header",
        ),
        (
            &*AWS_KEY,
            &with_boundaries,
            "aws-key",
            "AWS access key id pattern",
        ),
        (
            &*SECRET_ASSIGN,
            &with_boundaries,
            "secret",
            "secret-like assignment",
        ),
        (
            &*ABS_USER_PATH,
            text,
            "abs-user-path",
            "absolute user/home path leak",
        ),
        (
            &*PROMPT_INJECTION,
            &with_boundaries,
            "prompt-injection",
            "prompt-injection instruction",
        ),
        (
            &*OUTPUT_SUPPRESSION,
            &with_boundaries,
            "output-suppression",
            "output suppression instruction",
        ),
        (
            &*DESTRUCTIVE,
            &with_boundaries,
            "destructive-command",
            "destructive shell command",
        ),
        (
            &*FETCH_SECRET,
            &with_boundaries,
            "suspicious-fetch",
            "network command mentions credential material",
        ),
    ];
    for (pattern, haystack, id, message) in checks {
        if pattern.is_match(haystack) {
            report(id, message);
        }
    }
}
