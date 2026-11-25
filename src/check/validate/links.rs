use std::path::Path;

use pulldown_cmark::{Event, Parser, Tag};

use crate::check::frontmatter::python_whitespace;

pub(super) fn warn_missing(path: &Path, body: &str, warnings: &mut Vec<String>) {
    for event in Parser::new(body) {
        let (Event::Start(Tag::Link { dest_url, .. }) | Event::Start(Tag::Image { dest_url, .. })) =
            event
        else {
            continue;
        };
        let reference = dest_url.trim_matches(python_whitespace);
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
        let target = Path::new(clean);
        if !clean.is_empty() && !target.has_root() && !path.join(target).exists() {
            warnings.push(format!(
                "Referenced supporting file not found: {reference}. If you want the link to work, add the file or update the link."
            ));
        }
    }
}
