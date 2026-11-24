use std::sync::LazyLock;

use regex::Regex;

use super::analyzers::FileContext;
use super::patterns::{compile, is_python_word};
use super::{AuditFinding, rules::finding};

static TRAVERSAL: LazyLock<Regex> =
    LazyLock::new(|| compile(r#"^(?:\.\./)+[^\s\x1c-\x1f)>\]'"]*"#));

pub(super) fn analyze(context: &FileContext, findings: &mut Vec<AuditFinding>) {
    let Some(text) = context.text.as_deref() else {
        return;
    };
    // This is the Python lookbehind (?<![\w/]). A rejected candidate must
    // leave the rest of the text eligible for matching.
    let mut end = 0;
    for (start, _) in text.match_indices("../") {
        if start < end || text[..start].ends_with(|ch| ch == '/' || is_python_word(ch)) {
            continue;
        }
        let target = TRAVERSAL.find(&text[start..]).expect("traversal prefix");
        end = start + target.end();
        findings.push(finding(
            "path-traversal",
            &context.file,
            format!(
                "relative path escapes the skill directory: {}",
                target.as_str()
            ),
            None,
        ));
    }
}
