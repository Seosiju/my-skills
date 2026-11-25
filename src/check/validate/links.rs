use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use crate::check::frontmatter::python_whitespace;

static BACKTICKS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"`+").unwrap());
static LIST_MARKER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:[-+*]|[0-9]{1,9}[.)])[ \t]+").unwrap());

pub(super) fn warn_missing(path: &Path, body: &str, warnings: &mut Vec<String>) {
    let prose = without_fenced_code(body);
    for paragraph in prose.split("\n\n") {
        let prose = without_inline_code(paragraph);
        let mut remaining = prose.as_str();
        while let Some((_, rest)) = remaining.split_once("](") {
            let Some((reference, rest)) = rest.split_once(')') else {
                break;
            };
            remaining = rest;
            let reference = reference.trim_matches(python_whitespace);
            if ["http://", "https://", "#", "mailto:"]
                .iter()
                .any(|prefix| reference.starts_with(prefix))
            {
                continue;
            }
            let clean = reference
                .split(['#', '?'])
                .next()
                .unwrap_or_default()
                .trim_matches(python_whitespace);
            if !clean.is_empty() && !clean.starts_with('/') && !path.join(clean).exists() {
                warnings.push(format!(
                    "Referenced supporting file not found: {reference}. If you want the link to work, add the file or update the link."
                ));
            }
        }
    }
}

fn without_fenced_code(body: &str) -> String {
    let mut prose = String::new();
    let mut fence = None;
    for line in body.lines() {
        let mut content = line;
        let mut quote_depth = 0;
        while let Some(rest) = content.trim_start().strip_prefix('>') {
            quote_depth += 1;
            content = rest.strip_prefix(' ').unwrap_or(rest);
        }
        let trimmed = content.trim_start_matches([' ', '\t']);
        let indent = content[..content.len() - trimmed.len()]
            .bytes()
            .fold(0, |column, byte| {
                if byte == b'\t' {
                    column + (4 - column % 4)
                } else {
                    column + 1
                }
            });
        if fence.is_some_and(|(_, _, depth, list_indent)| {
            quote_depth < depth
                || (quote_depth == depth && !trimmed.is_empty() && indent < list_indent)
        }) {
            fence = None;
        }
        if let Some((marker, width, depth, list_indent)) = fence {
            let rest = trimmed.trim_start_matches(marker);
            if quote_depth == depth
                && indent <= list_indent + 3
                && trimmed.len() - rest.len() >= width
                && rest.trim_matches([' ', '\t']).is_empty()
            {
                fence = None;
            }
            continue;
        }
        let list_prefix = LIST_MARKER.find(trimmed).map_or(0, |marker| marker.end());
        let content = &trimmed[list_prefix..];
        if let Some(marker @ ('`' | '~')) = content.chars().next() {
            let rest = content.trim_start_matches(marker);
            let width = content.len() - rest.len();
            if width >= 3 && (marker == '~' || !rest.contains(marker)) {
                let list_indent = if list_prefix == 0 {
                    0
                } else {
                    indent + list_prefix
                };
                fence = Some((marker, width, quote_depth, list_indent));
                prose.push_str("\n\n");
                continue;
            }
        }
        if line.trim().is_empty() {
            prose.push('\n');
        } else {
            prose.push_str(line);
            prose.push('\n');
        }
    }
    prose
}

fn without_inline_code(paragraph: &str) -> String {
    let runs: Vec<_> = BACKTICKS.find_iter(paragraph).collect();
    let mut cursor = 0;
    let mut prose = String::new();
    for (index, opening) in runs.iter().enumerate() {
        if opening.start() < cursor {
            continue;
        }
        let backslashes = paragraph.as_bytes()[..opening.start()]
            .iter()
            .rev()
            .take_while(|&&byte| byte == b'\\')
            .count();
        // A backslash escapes just the first backtick of a run.
        let start = opening.start() + backslashes % 2;
        let width = opening.end() - start;
        if width > 0
            && let Some(closing) = runs[index + 1..].iter().find(|run| run.len() == width)
        {
            prose.push_str(&paragraph[cursor..start]);
            // Keep adjacent punctuation from forming a new link marker.
            prose.push('\u{fffc}');
            cursor = closing.end();
        }
    }
    prose.push_str(&paragraph[cursor..]);
    prose
}
