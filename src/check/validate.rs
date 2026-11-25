//! Skill format and host display diagnostics. Security gates are applied separately.

mod links;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::{fs, io};

use crate::check::frontmatter::{FrontmatterError, Mapping, Node, load_yaml, split_document};
use crate::drive::hosts::{HostConfig, MissingDescription};

const MAX_NAME_LEN: usize = 64;
const OPTIONAL_FIELDS: &[&str] = &[
    "license",
    "compatibility",
    "metadata",
    "allowed-tools",
    "allowed_tools",
];

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
            return result;
        }
    };

    let mut commented_fields = Vec::new();
    match load_yaml(&yaml, Some(&mut commented_fields)) {
        Ok(metadata) => {
            for (field, value) in commented_fields {
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
            if get(&metadata, "allowed-tools").is_some()
                || get(&metadata, "allowed_tools").is_some()
            {
                result.warnings.push(
                    "'allowed-tools' has inconsistent host support and may be ignored".into(),
                );
            }
            host_diagnostics(&metadata, &readable_hosts, &mut result);
        }
        Err(error) => warn_frontmatter(error, &mut result),
    }
    links::warn_missing(path, &body, &mut result.warnings);
    result
}

fn warn_frontmatter(error: FrontmatterError, result: &mut ValidationResult) {
    result.warnings.push(format!(
        "my-skills could not read the YAML frontmatter. If you want its fields checked, fix the YAML. {error}"
    ));
}

fn host_diagnostics(metadata: &Mapping, hosts: &[&HostConfig], result: &mut ValidationResult) {
    let name = field_text(metadata, "name");
    let description = field_text(metadata, "description");
    for host in hosts {
        let service = host.display_name;
        let display = &host.skill_display;
        if let Some(name) = &name {
            let length = name.chars().count();
            if let Some(limit) = display.name_limit
                && length > limit
            {
                result.warnings.push(format!(
                    "{service} does not show this skill because its name is {length} characters. If you want it to appear, use a name of {limit} characters or fewer."
                ));
                continue;
            }
        }

        if description.is_none() && display.missing_description == MissingDescription::Hidden {
            result.warnings.push(format!(
                "{service} does not show this skill because its description is missing or empty. If you want it to appear, add a description to the YAML frontmatter."
            ));
            continue;
        }

        if let Some(name) = &name
            && name != &result.skill
        {
            let folder = &result.skill;
            let (shown, source) = if display.uses_folder_name {
                (folder, "the folder name")
            } else {
                (name, "the YAML name")
            };
            result.warnings.push(format!(
                "{service} lists this skill as {shown:?} using {source}. The folder is {folder:?} and the YAML name is {name:?}. If you want the same name across services, make the two names match."
            ));
        }

        if let Some(description) = &description {
            let extra_length = display
                .description_limit
                .and_then(|limit| limit.extra_field)
                .and_then(|field| field_text(metadata, field))
                .map_or(0, |extra| extra.chars().count());
            if let Some(warning) =
                description_warning(host, description.chars().count() + extra_length)
            {
                result.warnings.push(warning);
            }
        } else {
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
                    if OPTIONAL_FIELDS.contains(&key.as_str())
                        || display.frontmatter_keep_fields.contains(&key.as_str()))
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

pub(crate) fn description_warning(host: &HostConfig, length: usize) -> Option<String> {
    let limit = host.skill_display.description_limit?;
    if length <= limit.max_chars {
        return None;
    }
    let subject = limit.extra_field.map_or_else(
        || "description".to_owned(),
        |field| format!("description and {field} combined"),
    );
    let visible = limit.max_chars - limit.suffix.chars().count();
    Some(format!(
        "{} shows a shortened {subject} ({length} characters). It keeps the first {visible} characters and adds {:?}. If you want the full text to appear, keep it to {} characters or fewer.",
        host.display_name, limit.suffix, limit.max_chars
    ))
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

/// Content diagnostics plus a security
/// audit with every rule enabled. Findings at `High` or above become errors
/// (`security: <file>: <message>`); lower findings become warnings.
pub fn compose_validation(path: &Path) -> ValidationResult {
    compose_validation_for_hosts(path, &[])
}

fn compose_validation_for_hosts(path: &Path, hosts: &[&HostConfig]) -> ValidationResult {
    use crate::check::audit::{AuditPolicy, Severity, run_audit};

    let mut result = validate(path, hosts);
    let policy = AuditPolicy {
        enabled: true,
        profile: "default".into(),
        threshold: None,
        disabled_rules: Vec::new(),
    };
    match run_audit(path, &policy) {
        Ok(audit) => {
            for finding in audit.findings {
                let line = format!("security: {}: {}", finding.file, finding.message);
                if finding.severity >= Severity::High {
                    result.errors.push(line);
                } else {
                    result.warnings.push(line);
                }
            }
        }
        Err(err) => result.errors.push(format!("security: audit failed: {err}")),
    }
    result
}

/// The drive's commit and health gate: standard validation plus the fixed
/// security floor (`compose_validation`), the host checks for every enabled
/// target the skill connects to, and the manifest's `[audit]` policy
/// (profile, threshold, disabled rules). Used by the pre-commit hook and by
/// `doctor`'s full validation so both apply the same rules (spec D2).
/// `manifest = None` (unreadable manifest) falls back to built-in host
/// defaults and the default audit policy.
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
    use crate::check::audit::{policy_from_settings, run_audit};
    use crate::registry::manifest::AuditSettings;

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
    let mut result = compose_validation_for_hosts(path, &hosts);

    let settings = manifest.map_or_else(AuditSettings::default, |m| m.audit.clone());
    match policy_from_settings(&settings) {
        Ok(policy) if policy.enabled => match run_audit(path, &policy) {
            Ok(audit) => {
                if let Some(threshold) = audit.threshold {
                    for finding in audit.findings.iter().filter(|f| f.severity >= threshold) {
                        let floor = format!("security: {}: {}", finding.file, finding.message);
                        result.warnings.retain(|warning| warning != &floor);
                        if !result.errors.contains(&floor) {
                            result.errors.push(format!(
                                "audit ({}): {}: {}: {}",
                                finding.severity.label(),
                                finding.rule_id,
                                finding.file,
                                finding.message
                            ));
                        }
                    }
                }
            }
            Err(err) => result.errors.push(format!("audit failed: {err}")),
        },
        Ok(_) => {}
        Err(err) => result.errors.push(format!("audit policy: {err}")),
    }
    result
}

#[cfg(test)]
mod compose_tests {
    use super::*;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("shared")
            .join(name)
    }

    #[test]
    fn compose_validation_keeps_the_security_floor_and_allows_content() {
        let good = compose_validation(&fixture("good-skill"));
        assert!(
            good.errors.is_empty() && good.warnings.is_empty(),
            "{good:?}"
        );

        let secret = compose_validation(&fixture("secret-skill"));
        assert_eq!(
            secret.errors,
            vec![
                "security: SKILL.md: AWS access key id pattern",
                "security: SKILL.md: secret-like assignment",
            ]
        );

        let bad = compose_validation(&fixture("bad-name"));
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
