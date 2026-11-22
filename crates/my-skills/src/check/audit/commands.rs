use std::sync::LazyLock;

use regex::Regex;

use super::analyzers::FileContext;
use super::patterns::{boundary_text, compile, line_number};
use super::{AuditFinding, rules::finding};

static CREDENTIAL_READER: LazyLock<Regex> = LazyLock::new(|| {
    compile(concat!(
        r"(?ix)",
        r"\b(?:cat|less|grep|awk|sed)\b[^\n\x60]*",
        r"(?:~|\$HOME|/Users/[^/\s\x1c-\x1f]+|/home/[^/\s\x1c-\x1f]+)?/",
        r"(?:\.aws/credentials|\.ssh/id_rsa|\.config/gcloud|\.docker/config\.json)",
        r"|\bgh[\s\x1c-\x1f]+auth[\s\x1c-\x1f]+token\b",
        r"|\bsecurity[\s\x1c-\x1f]+find-(?:generic|internet)-password\b",
        r"|\bop[\s\x1c-\x1f]+read\b",
    ))
});

static NETWORK_SENDER: LazyLock<Regex> = LazyLock::new(|| {
    compile(concat!(
        r"(?ix)",
        r"\b(?:curl|wget)\b[^\n\x60]*https?://",
        r"|\b(?:nc|netcat)\b[^\n\x60]*(?:\d{1,3}(?:\.\d{1,3}){3}|[a-z0-9.-]+\.[a-z]{2,})",
        r"|\b(?:scp|rsync|ssh)\b[^\n\x60]*(?:[a-z0-9._-]+@|[a-z0-9.-]+\.[a-z]{2,})",
    ))
});

pub(super) fn analyze(context: &FileContext, findings: &mut Vec<AuditFinding>) {
    let Some(text) = context.text.as_deref() else {
        return;
    };
    let text = boundary_text(text);
    for (pattern, rule_id, message) in [
        (
            &*CREDENTIAL_READER,
            "credential-reader",
            "command reads credential material",
        ),
        (
            &*NETWORK_SENDER,
            "network-sender",
            "command can send data over the network",
        ),
    ] {
        findings.extend(pattern.find_iter(&text).map(|matched| {
            finding(
                rule_id,
                &context.file,
                message,
                Some(line_number(&text, matched.start())),
            )
        }));
    }
}

pub(super) fn capabilities(text: &str) -> (bool, bool) {
    let text = boundary_text(text);
    (
        CREDENTIAL_READER.is_match(&text),
        NETWORK_SENDER.is_match(&text),
    )
}
