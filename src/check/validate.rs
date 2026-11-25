//! Skill format and host display diagnostics.

mod links;

use std::borrow::Cow;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::{fs, io};

use crate::check::frontmatter::{
    FrontmatterError, Mapping, Node, YamlDiagnostics, load_yaml, split_document,
};
use crate::drive::hosts::{HostConfig, MissingDescription};

const MAX_NAME_LEN: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationResult {
    pub skill: String,
    pub path: PathBuf,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

impl ValidationResult {
    pub fn ok(&self) -> bool {
        self.errors.is_empty()
    }
}

/// Error strings for an invalid skill name; empty when valid.
pub fn validate_name(name: &str) -> Vec<String> {
    let mut errors = Vec::new();
    if name.chars().count() > MAX_NAME_LEN {
        errors.push(format!("name exceeds {MAX_NAME_LEN} characters"));
    }
    // Python's regex $ also permits a match just before one final newline.
    let words = name.strip_suffix('\n').unwrap_or(name);
    if !words.split('-').all(|word| {
        !word.is_empty()
            && word
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
    }) {
        errors.push(
            "name must be lowercase alphanumeric words separated by single \
             hyphens (no leading, trailing, or consecutive hyphens)"
                .into(),
        );
    }
    errors
}

/// Content diagnostics without a specific target host.
pub fn validate_skill(path: &Path) -> ValidationResult {
    validate(path, &[])
}

/// Content diagnostics with display warnings for a specific host.
pub fn validate_skill_for_host(path: &Path, host: &HostConfig) -> ValidationResult {
    validate(path, &[host])
}

fn validate(path: &Path, hosts: &[&HostConfig]) -> ValidationResult {
    let skill = if path.ends_with("..") {
        "..".into()
    } else {
        path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    };
    let mut result = ValidationResult {
        skill,
        path: path.to_owned(),
        errors: Vec::new(),
        warnings: Vec::new(),
    };
    let skill_md = path.join("SKILL.md");
    let text = match fs::metadata(&skill_md).and_then(|metadata| {
        if metadata.is_file() {
            fs::read_to_string(skill_md)
        } else {
            Err(io::ErrorKind::NotFound.into())
        }
    }) {
        Ok(text) => text,
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
            ) =>
        {
            result.errors.push("SKILL.md not found".into());
            return result;
        }
        Err(error) => {
            result.warnings.push(format!(
                "my-skills could not read SKILL.md: {error}. If you want it checked, make sure the file is readable UTF-8."
            ));
            return result;
        }
    };

    let mut readable_hosts = Vec::new();
    for &host in hosts {
        if text.starts_with('\u{feff}') && !host.skill_display.accepts_bom {
            result.warnings.push(format!(
                "{} does not show this skill because SKILL.md starts with a UTF-8 BOM. If you want it to appear, save the file as UTF-8 without a BOM.",
                host.display_name
            ));
        } else {
            readable_hosts.push(host);
        }
    }

    let (yaml, body) = match split_document(&text) {
        Ok(Some(document)) => document,
        Ok(None) => {
            for host in readable_hosts {
                if host.skill_display.requires_frontmatter {
                    result.warnings.push(format!(
                        "{} does not show this skill because SKILL.md has no YAML frontmatter. If you want it to appear, add a description in a YAML block between '---' lines at the top.",
                        host.display_name
                    ));
                } else {
                    result.warnings.push(format!(
                        "{} uses the first body line as the description because SKILL.md has no YAML frontmatter. If you want a different description, add one in a YAML block at the top.",
                        host.display_name
                    ));
                }
            }
            links::warn_missing(path, &text, &mut result.warnings);
            return result;
        }
        Err(error) => {
            warn_frontmatter(error, &mut result);
            for host in readable_hosts {
                let service = host.display_name;
                let warning = if host.skill_display.requires_frontmatter {
                    format!(
                        "{service} does not show this skill because the YAML frontmatter has no closing '---'. If you want it to appear, add the closing line."
                    )
                } else {
                    format!(
                        "{service} may treat SKILL.md as body text because the YAML frontmatter has no closing '---'. If you want it to use the YAML fields, add the closing line."
                    )
                };
                result.warnings.push(warning);
            }
            return result;
        }
    };

    let mut diagnostics = YamlDiagnostics::default();
    match load_yaml(&yaml, Some(&mut diagnostics)) {
        Ok(metadata) => {
            for (field, value) in &diagnostics.commented_fields {
                const PREVIEW_LEN: usize = 48;
                let mut chars = value.chars();
                let mut preview: String = chars.by_ref().take(PREVIEW_LEN).collect();
                if chars.next().is_some() {
                    preview.push('…');
                }
                result.warnings.push(format!(
                    "A YAML comment cuts off '{field}'. Agents see {preview:?}. If you want the full value to appear, put it in quotes."
                ));
            }
            host_diagnostics(&metadata, &diagnostics, &readable_hosts, &mut result);
        }
        Err(error) => {
            warn_frontmatter(error, &mut result);
            for host in readable_hosts {
                let service = host.display_name;
                let warning = if host.skill_display.malformed_frontmatter_uses_body {
                    format!(
                        "If {service} cannot parse the YAML frontmatter, it loads the skill body without the YAML fields. If you want it to use those fields, fix the YAML."
                    )
                } else {
                    format!(
                        "{service} may not show this skill if it cannot read the YAML frontmatter. If you want to make it easier to load, fix the YAML."
                    )
                };
                result.warnings.push(warning);
            }
        }
    }
    links::warn_missing(path, &body, &mut result.warnings);
    result
}

fn warn_frontmatter(error: FrontmatterError, result: &mut ValidationResult) {
    result.warnings.push(format!(
        "my-skills could not read the YAML frontmatter. If you want its fields checked, fix the YAML. {error}"
    ));
}

fn host_diagnostics(
    metadata: &Mapping,
    diagnostics: &YamlDiagnostics,
    hosts: &[&HostConfig],
    result: &mut ValidationResult,
) {
    let name_node = get(metadata, "name");
    let legacy_name = field_text(metadata, "name");
    let description_node = get(metadata, "description");
    let name_is_collection = is_yaml_collection(name_node);
    let description_is_collection = is_yaml_collection(description_node);
    for host in hosts {
        let service = host.display_name;
        let display = &host.skill_display;
        let rejects_as_text = |node: Option<&Node>, may_be_quoted: bool| {
            display.expects_text_name_and_description
                && matches!(node, Some(Node::List(_) | Node::Map(_)))
                && !may_be_quoted
        };
        let uncertain_name = display.expects_text_name_and_description && name_is_collection;
        let name = if uncertain_name {
            if rejects_as_text(name_node, diagnostics.name_may_be_quoted) {
                result.warnings.push(format!(
                    "{service} does not show this skill because its YAML name is a list or mapping. If you want it to appear, use a plain text name."
                ));
                continue;
            }
            result.warnings.push(format!(
                "{service} may display the name differently or omit the skill when the YAML name is a list or mapping. If you want a predictable name, use a plain text name."
            ));
            None
        } else if display.expects_text_name_and_description {
            diagnostics.plain_name.as_deref().or(legacy_name.as_deref())
        } else {
            legacy_name.as_deref()
        };
        let yaml_name = name
            .map(|name| display_text(host, name))
            .filter(|name| !name.is_empty());
        let folder = &result.skill;
        let shown = if display.uses_folder_name {
            folder.as_str()
        } else {
            yaml_name.as_deref().unwrap_or(folder)
        };
        let length = shown.chars().count();
        if let Some(limit) = display.name_limit
            && !uncertain_name
            && length > limit
        {
            let warning = if yaml_name.is_none() {
                format!(
                    "{service} does not show this skill because it uses the {length}-character folder name. If you want it to appear, use a folder name or YAML name of {limit} characters or fewer."
                )
            } else {
                format!(
                    "{service} does not show this skill because its name is {length} characters. If you want it to appear, use a name of {limit} characters or fewer."
                )
            };
            result.warnings.push(warning);
            continue;
        }

        // Codex accepts plain scalars such as numbers and booleans as text.
        let description = description_node
            .filter(|node| !description_is_collection && !matches!(node, Node::Null))
            .filter(|node| display.expects_text_name_and_description || node.truthy())
            .map(Node::python_string)
            .filter(|text| !display.collapse_whitespace || !text.trim().is_empty());
        if description_is_collection {
            let rejects_type =
                rejects_as_text(description_node, diagnostics.description_may_be_quoted);
            let warning = if rejects_type {
                format!(
                    "{service} does not show this skill because its YAML description is a list or mapping. If you want it to appear, use a plain text description."
                )
            } else if display.missing_description == MissingDescription::FirstBodyLine {
                format!(
                    "The YAML description is a list or mapping. {service} uses the first non-empty body line when a description is omitted. If you want a custom description, write the YAML value as text."
                )
            } else {
                format!(
                    "{service} may display the description differently or omit the skill when the YAML description is a list or mapping. If you want a text description, put it in quotes or use a YAML text block."
                )
            };
            result.warnings.push(warning);
            if rejects_type {
                continue;
            }
        } else if description.is_none() && display.missing_description == MissingDescription::Hidden
        {
            result.warnings.push(format!(
                "{service} does not show this skill because its description is missing or empty. If you want it to appear, add a description to the YAML frontmatter."
            ));
            continue;
        }

        if let Some(name) = name
            && (name != folder || shown != folder)
        {
            let source = if display.uses_folder_name || yaml_name.is_none() {
                "the folder name"
            } else {
                "the YAML name"
            };
            result.warnings.push(format!(
                "{service} lists this skill as {shown:?} using {source}. The folder is {folder:?} and the YAML name is {name:?}. If you want the same name across services, make the two names match."
            ));
        }

        if let Some(description) = &description {
            let extra = display
                .description_limit
                .and_then(|limit| limit.extra_field)
                .and_then(|field| field_text(metadata, field));
            if let Some(warning) = description_warning(host, description, extra.as_deref()) {
                result.warnings.push(warning);
            }
        } else if !description_is_collection
            && display.missing_description == MissingDescription::Blank
            && description_node.is_some_and(|node| !matches!(node, Node::Null | Node::String(_)))
        {
            result.warnings.push(format!(
                "The YAML description is a non-text scalar. How {service} displays it may vary. If you want a text description, put the value in quotes."
            ));
        } else if !description_is_collection {
            let message = match display.missing_description {
                MissingDescription::FirstBodyLine => "uses the first body line as the description",
                MissingDescription::Blank => "shows an empty description",
                MissingDescription::Hidden => unreachable!("hidden skill"),
            };
            result.warnings.push(format!(
                "{service} {message} because the YAML description is missing or empty. If you want a custom description, add one to the YAML frontmatter."
            ));
        }

        let fields: BTreeSet<_> = metadata
            .iter()
            .filter(|(key, _)| {
                !matches!(&**key, Node::String(key)
                    if display.frontmatter_keep_fields.contains(&key.as_str()))
            })
            .map(|(key, _)| key.python_string())
            .collect();
        if !fields.is_empty() {
            let fields = fields.into_iter().collect::<Vec<_>>().join(", ");
            result.warnings.push(format!(
                "{service} ignores these YAML fields: {fields}. If you want {service} to see that information, put it in a supported field or the skill body."
            ));
        }
    }
}

fn is_yaml_collection(node: Option<&Node>) -> bool {
    matches!(
        node,
        Some(Node::List(_) | Node::Map(_) | Node::Set(_) | Node::Pair(..))
    )
}

pub(crate) fn description_warning(
    host: &HostConfig,
    description: &str,
    extra: Option<&str>,
) -> Option<String> {
    let limit = host.skill_display.description_limit?;
    let extra = limit
        .extra_field
        .zip(extra.filter(|text| !text.trim().is_empty()));
    let length = display_text(host, description).chars().count()
        + extra.map_or(0, |(_, text)| display_text(host, text).chars().count());
    if length <= limit.max_chars {
        return None;
    }
    let subject = extra.map_or_else(
        || "description".to_owned(),
        |(field, _)| format!("description and {field} combined"),
    );
    let visible = limit.max_chars.saturating_sub(limit.suffix.chars().count());
    Some(format!(
        "{} shows a shortened {subject} ({length} characters). It keeps the first {visible} characters and adds {:?}. If you want the full text to appear, keep it to {} characters or fewer.",
        host.display_name, limit.suffix, limit.max_chars
    ))
}

fn display_text<'a>(host: &HostConfig, text: &'a str) -> Cow<'a, str> {
    if host.skill_display.collapse_whitespace {
        Cow::Owned(text.split_whitespace().collect::<Vec<_>>().join(" "))
    } else {
        Cow::Borrowed(text)
    }
}

fn field_text(metadata: &Mapping, name: &str) -> Option<String> {
    get(metadata, name)
        .filter(|node| node.truthy())
        .map(Node::python_string)
}

fn get<'a>(metadata: &'a Mapping, name: &str) -> Option<&'a Node> {
    metadata.iter().find_map(|(key, value)| {
        matches!(&**key, Node::String(key) if key == name).then_some(value.as_ref())
    })
}

/// Errors used by the pre-commit hook and doctor's full validation.
pub fn gate_errors(
    path: &Path,
    name: &str,
    manifest: Option<&crate::registry::manifest::Manifest>,
) -> Vec<String> {
    gate_validation(path, name, manifest, None).errors
}

/// The full gate result for the CLI. An explicit host replaces the default
/// enabled and listed host filter, while all other gate rules still apply.
pub fn gate_validation(
    path: &Path,
    name: &str,
    manifest: Option<&crate::registry::manifest::Manifest>,
    only_host: Option<&str>,
) -> ValidationResult {
    let skill = manifest.and_then(|m| m.skills.get(name));
    let hosts = crate::drive::hosts::ALL
        .into_iter()
        .filter(|host| {
            let enabled = manifest
                .and_then(|m| m.targets.get(host.name))
                .map_or(host.default_enabled, |target| target.enabled);
            let listed =
                skill.is_none_or(|s| s.hosts.is_empty() || s.hosts.iter().any(|h| h == host.name));
            only_host.map_or(enabled && listed, |name| name == host.name)
        })
        .collect::<Vec<_>>();
    validate(path, &hosts)
}

#[cfg(test)]
mod validation_tests {
    use super::*;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("shared")
            .join(name)
    }

    #[test]
    fn validation_keeps_host_display_warnings() {
        let good = validate_skill(&fixture("good-skill"));
        assert!(
            good.errors.is_empty() && good.warnings.is_empty(),
            "{good:?}"
        );

        let bad = validate_skill(&fixture("bad-name"));
        assert_eq!(bad.errors, Vec::<String>::new());
        let codex = validate_skill_for_host(&fixture("bad-name"), &crate::drive::hosts::CODEX);
        assert_eq!(
            codex.warnings,
            [
                "Codex lists this skill as \"Bad--Name\" using the YAML name. The folder is \"bad-name\" and the YAML name is \"Bad--Name\". If you want the same name across services, make the two names match."
            ]
        );
    }
}
