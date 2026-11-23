use super::{AuditFinding, AuditResult, rules::finding};

/// Report the cross-skill finding for the CLI's multi-skill audit.
pub fn cross_skill_findings(results: &[AuditResult]) -> Vec<AuditFinding> {
    let readers = skills_with(results, "credential-reader");
    let senders = skills_with(results, "network-sender");
    if readers.is_empty() || senders.is_empty() {
        return Vec::new();
    }
    if readers
        .iter()
        .chain(&senders)
        .all(|name| *name == readers[0])
    {
        return Vec::new();
    }
    vec![finding(
        "cross-skill-credential-network",
        "<bundle>",
        format!(
            "selected skills combine credential readers ({}) with network senders ({})",
            readers.join(", "),
            senders.join(", "),
        ),
        None,
    )]
}

fn skills_with<'a>(results: &'a [AuditResult], rule_id: &str) -> Vec<&'a str> {
    let mut skills: Vec<_> = results
        .iter()
        .filter(|result| {
            result
                .findings
                .iter()
                .any(|finding| finding.rule_id == rule_id)
        })
        .map(|result| result.skill.as_str())
        .collect();
    skills.sort_unstable();
    skills
}
