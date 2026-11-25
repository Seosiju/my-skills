use std::path::Path;

use pulldown_cmark::{Event, LinkType, Parser, Tag};

use crate::check::frontmatter::python_whitespace;

pub(super) fn warn_missing(path: &Path, body: &str, warnings: &mut Vec<String>) {
    for event in Parser::new(body) {
        let dest_url = match event {
            Event::Start(Tag::Link {
                link_type: LinkType::Autolink | LinkType::Email,
                ..
            }) => continue,
            Event::Start(Tag::Link { dest_url, .. } | Tag::Image { dest_url, .. }) => dest_url,
            _ => continue,
        };
        let reference = dest_url.trim_matches(python_whitespace);
        if reference.starts_with('#') || has_uri_scheme(reference) {
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

fn has_uri_scheme(reference: &str) -> bool {
    reference.split_once(':').is_some_and(|(scheme, _)| {
        let mut bytes = scheme.bytes();
        bytes.next().is_some_and(|byte| byte.is_ascii_alphabetic())
            && bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'.' | b'-'))
    })
}
