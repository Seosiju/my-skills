use std::sync::LazyLock;

use regex::Regex;

use super::analyzers::FileContext;
use super::patterns::{compile, line_number};
use super::{AuditFinding, rules::finding};

static REMOTE_IMAGE: LazyLock<Regex> =
    LazyLock::new(|| compile(r"!\[[^\]]*\]\((https?://[^)\s\x1c-\x1f]+)\)"));

pub(super) fn analyze(context: &FileContext, findings: &mut Vec<AuditFinding>) {
    let Some(text) = context.text.as_deref() else {
        return;
    };
    if !context.file.to_lowercase().ends_with(".md") {
        return;
    }
    for captures in REMOTE_IMAGE.captures_iter(text) {
        let start = captures.get(0).expect("full image match").start();
        findings.push(finding(
            "markdown-remote-image",
            &context.file,
            format!("markdown image loads remote URL: {}", &captures[1]),
            Some(line_number(text, start)),
        ));
    }
}
