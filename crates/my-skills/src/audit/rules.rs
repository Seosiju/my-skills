use std::collections::BTreeMap;
use std::sync::LazyLock;

use yaml_rust2::YamlLoader;

use super::{AuditFinding, Severity};

static BUILTIN_RULES: LazyLock<BTreeMap<String, (String, Severity)>> = LazyLock::new(|| {
    let documents = YamlLoader::load_from_str(include_str!("builtin_rules.yaml"))
        .expect("valid embedded audit YAML");
    let rules = documents[0]["rules"].as_vec().expect("embedded rules list");
    rules
        .iter()
        .map(|rule| {
            let id = rule["id"].as_str().expect("audit rule id");
            let category = rule["category"].as_str().expect("audit rule category");
            let severity = rule["severity"].as_str().expect("audit rule severity");
            (
                id.to_owned(),
                (
                    category.to_owned(),
                    Severity::parse(severity).expect("valid built-in severity"),
                ),
            )
        })
        .collect()
});

pub(super) fn finding(
    rule_id: &str,
    file: &str,
    message: impl Into<String>,
    line: Option<u32>,
) -> AuditFinding {
    let (category, severity) = match rule_id {
        "nul-byte" | "encoding" => ("encoding", Severity::Critical),
        "audit-read-error" => ("scanner", Severity::Critical),
        _ => {
            let (category, severity) = &BUILTIN_RULES[rule_id];
            (category.as_str(), *severity)
        }
    };
    AuditFinding {
        rule_id: rule_id.into(),
        category: category.into(),
        severity,
        file: file.into(),
        message: message.into(),
        line,
    }
}
