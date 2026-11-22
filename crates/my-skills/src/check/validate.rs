//! Agent Skills standard validation. Stream D. Mirrors v0.4.1
//! `validation.py`. (v0.4.1 `checks.compose_validation`, which adds audit
//! findings, is wired up by the integrator once stream E lands.)

use std::fs;
use std::path::{Path, PathBuf};

use crate::check::frontmatter::{Mapping, Node, parse_document, python_whitespace};
use crate::drive::hosts::HostConfig;

const MAX_NAME_LEN: usize = 64;
const MAX_DESCRIPTION_LEN: usize = 1024;

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

/// Same checks and message text as v0.4.1 `validate_skill`.
pub fn validate_skill(path: &Path) -> ValidationResult {
    validate(path, None)
}

/// Same checks and message text as v0.4.1 `validate_skill_for_host`.
pub fn validate_skill_for_host(path: &Path, host: &HostConfig) -> ValidationResult {
    validate(path, Some(host))
}

fn validate(path: &Path, host: Option<&HostConfig>) -> ValidationResult {
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
    if !skill_md.is_file() {
        result.errors.push("SKILL.md not found".into());
        return result;
    }
    let text = match fs::read_to_string(skill_md) {
        Ok(text) => text,
        Err(error) => {
            result
                .errors
                .push(format!("could not read SKILL.md: {error}"));
            return result;
        }
    };
    let (metadata, body) = match parse_document(&text) {
        Ok(parsed) => parsed,
        Err(error) => {
            result.errors.push(error.0);
            return result;
        }
    };

    if let Some(name) = get(&metadata, "name").filter(|name| name.truthy()) {
        let name = name.python_string();
        result.errors.extend(validate_name(&name));
        if name != result.skill {
            result.errors.push(format!(
                "directory name '{}' does not match frontmatter name '{name}'",
                result.skill
            ));
        }
    } else {
        result
            .errors
            .push("frontmatter is missing required field 'name'".into());
    }

    let description_length = if let Some(description) =
        get(&metadata, "description").filter(|description| description.truthy())
    {
        let length = description.python_string().chars().count();
        if length > MAX_DESCRIPTION_LEN {
            result.errors.push(format!(
                "description exceeds {MAX_DESCRIPTION_LEN} characters ({length})"
            ));
        }
        length
    } else {
        result
            .errors
            .push("frontmatter is missing required field 'description'".into());
        0
    };

    if get(&metadata, "allowed-tools").is_some() || get(&metadata, "allowed_tools").is_some() {
        result
            .warnings
            .push("'allowed-tools' has inconsistent host support and may be ignored".into());
    }

    let mut remaining = body.as_str();
    // Match Python's \]\(([^)]+)\) even in image links and code spans.
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
            result
                .errors
                .push(format!("referenced supporting file not found: {reference}"));
        }
    }

    if let Some(host) = host.filter(|_| result.ok()) {
        if description_length > host.description_max_chars {
            result.errors.push(format!(
                "{} description exceeds {} characters",
                host.name, host.description_max_chars
            ));
        }
        let mut extra: Vec<_> = metadata.iter()
            .filter(|(key, _)| !matches!(&**key, Node::String(key) if host.frontmatter_keep_fields.contains(&key.as_str())))
            .map(|(key, _)| key.python_string())
            .collect();
        extra.sort();
        if !extra.is_empty() {
            result.warnings.push(format!(
                "{} may ignore unsupported frontmatter fields: {}",
                host.name,
                extra.join(", ")
            ));
        }
    }
    result
}

fn get<'a>(metadata: &'a Mapping, name: &str) -> Option<&'a Node> {
    metadata.iter().find_map(|(key, value)| {
        matches!(&**key, Node::String(key) if key == name).then_some(value.as_ref())
    })
}

/// v0.4.1 `checks.compose_validation`: standard validation plus a security
/// audit with every rule enabled. Findings at `High` or above become errors
/// (`security: <file>: <message>`); lower findings become warnings. Used by
/// `validate`, `install`/`sync`, and the pre-commit hook.
pub fn compose_validation(path: &Path) -> ValidationResult {
    use crate::check::audit::{AuditPolicy, Severity, run_audit};

    let mut result = validate_skill(path);
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
    use crate::check::audit::{policy_from_settings, run_audit};
    use crate::registry::manifest::AuditSettings;

    let mut errors = compose_validation(path).errors;
    let skill = manifest.and_then(|m| m.skills.get(name));
    for host in crate::drive::hosts::ALL {
        let enabled = manifest
            .and_then(|m| m.targets.get(host.name))
            .map_or(host.default_enabled, |target| target.enabled);
        let listed =
            skill.is_none_or(|s| s.hosts.is_empty() || s.hosts.iter().any(|h| h == host.name));
        if enabled && listed {
            for error in validate_skill_for_host(path, host).errors {
                if !errors.contains(&error) {
                    errors.push(error);
                }
            }
        }
    }

    let settings = manifest.map_or_else(AuditSettings::default, |m| m.audit.clone());
    match policy_from_settings(&settings) {
        Ok(policy) if policy.enabled => match run_audit(path, &policy) {
            Ok(result) => {
                if let Some(threshold) = result.threshold {
                    for finding in result.findings.iter().filter(|f| f.severity >= threshold) {
                        let floor = format!("security: {}: {}", finding.file, finding.message);
                        if !errors.contains(&floor) {
                            errors.push(format!(
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
            Err(err) => errors.push(format!("audit failed: {err}")),
        },
        Ok(_) => {}
        Err(err) => errors.push(format!("audit policy: {err}")),
    }
    errors
}

#[cfg(test)]
mod compose_tests {
    use super::*;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("tests")
            .join("fixtures")
            .join(name)
    }

    #[test]
    fn matches_v041_compose_validation() {
        // Expected values printed by v0.4.1 `checks.compose_validation`.
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
        assert_eq!(
            bad.errors,
            vec![
                "name must be lowercase alphanumeric words separated by single hyphens (no leading, trailing, or consecutive hyphens)",
                "directory name 'bad-name' does not match frontmatter name 'Bad--Name'",
            ]
        );
    }
}
