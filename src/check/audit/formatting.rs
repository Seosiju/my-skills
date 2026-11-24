use std::fmt::Write;

use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::{AuditFinding, AuditGateResult, AuditResult, Severity};

// Field order is significant for Python's compact, sorted-key hash input.
#[derive(Serialize)]
struct FindingPayload<'a> {
    category: &'a str,
    file: &'a str,
    line: Option<u32>,
    message: &'a str,
    rule_id: &'a str,
    severity: &'static str,
}

impl<'a> From<&'a AuditFinding> for FindingPayload<'a> {
    fn from(finding: &'a AuditFinding) -> Self {
        Self {
            category: &finding.category,
            file: &finding.file,
            line: finding.line,
            message: &finding.message,
            rule_id: &finding.rule_id,
            severity: finding.severity.label(),
        }
    }
}

pub(super) fn result_hash(findings: &[AuditFinding]) -> String {
    let payload: Vec<_> = findings.iter().map(FindingPayload::from).collect();
    let json = serde_json::to_string(&payload).expect("finding JSON is serializable");
    let mut ascii = String::with_capacity(json.len());
    for ch in json.chars() {
        if ch < '\u{7f}' {
            ascii.push(ch);
        } else {
            for unit in ch.encode_utf16(&mut [0; 2]) {
                write!(ascii, r"\u{unit:04x}").expect("writing to a String cannot fail");
            }
        }
    }
    format!("sha256:{:x}", Sha256::digest(ascii.as_bytes()))
}

/// The v0.4.1 CLI representation of a finding. The line is optional.
pub fn finding_json(finding: &AuditFinding) -> Value {
    let mut value = json!(FindingPayload::from(finding));
    if finding.line.is_none() {
        value
            .as_object_mut()
            .expect("finding JSON is an object")
            .remove("line");
    }
    value
}

/// The v0.4.1 CLI representation of a single skill's audit.
pub fn result_json(result: &AuditResult) -> Value {
    json!({
        "skill": result.skill,
        // Display only; the result_hash input above never includes the path.
        "path": crate::platform::paths::display_path(&result.root),
        "profile": result.profile,
        "threshold": threshold_label(result.threshold),
        "blocked": result.blocked(),
        "result_hash": result.result_hash(),
        "findings": result.findings.iter().map(finding_json).collect::<Vec<_>>(),
    })
}

pub fn gate_json(gate: &AuditGateResult) -> Value {
    json!({
        "skipped": gate.skipped,
        "blocked": gate.blocked(),
        "skills": gate.results.iter().map(result_json).collect::<Vec<_>>(),
    })
}

pub fn format_gate(gate: &AuditGateResult, blocked_title: &str) -> String {
    if gate.skipped {
        return "WARN: audit skipped by explicit --skip-audit".into();
    }
    if gate.results.is_empty() {
        return "Audit: no skills scanned".into();
    }

    let mut output = if gate.blocked() {
        blocked_title.to_owned()
    } else {
        "Audit: passed".into()
    };
    for result in &gate.results {
        let status = if result.blocked() { "blocked" } else { "ok" };
        let threshold = threshold_label(result.threshold);
        write!(
            output,
            "\n  {}: {status} (threshold={threshold})",
            result.skill
        )
        .expect("writing to a String cannot fail");
        for finding in &result.findings {
            write!(
                output,
                "\n    {}: {}: {}: {}",
                finding.severity.label(),
                finding.rule_id,
                finding.file,
                finding.message
            )
            .expect("writing to a String cannot fail");
        }
    }
    output
}

fn threshold_label(threshold: Option<Severity>) -> &'static str {
    threshold.map_or("none", Severity::label)
}
