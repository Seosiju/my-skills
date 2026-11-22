//! Skill security audit, including the v0.4.1 built-in rules.

mod analyzers;
mod bundle;
mod commands;
mod formatting;
mod markdown;
mod patterns;
mod rules;
mod static_checks;
mod structure;

use std::path::{Component, Path, PathBuf};

use crate::manifest::AuditSettings;
use crate::{Error, Result};

pub use bundle::cross_skill_findings;
pub use formatting::{finding_json, format_gate, gate_json, result_json};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    Info = 10,
    Low = 20,
    Medium = 30,
    High = 40,
    Critical = 50,
}

impl Severity {
    /// Lowercase label, as v0.4.1 prints it.
    pub fn label(self) -> &'static str {
        match self {
            Severity::Info => "info",
            Severity::Low => "low",
            Severity::Medium => "medium",
            Severity::High => "high",
            Severity::Critical => "critical",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value
            .trim_matches(|ch: char| ch.is_whitespace() || matches!(ch, '\u{1c}'..='\u{1f}'))
            .to_uppercase()
            .as_str()
        {
            "INFO" => Ok(Self::Info),
            "LOW" => Ok(Self::Low),
            "MEDIUM" => Ok(Self::Medium),
            "HIGH" => Ok(Self::High),
            "CRITICAL" => Ok(Self::Critical),
            _ => Err(Error::Manifest(format!("unknown audit severity: {value}"))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditFinding {
    pub rule_id: String,
    pub category: String,
    pub severity: Severity,
    pub file: String,
    pub message: String,
    pub line: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditResult {
    pub skill: String,
    pub root: PathBuf,
    pub findings: Vec<AuditFinding>,
    pub profile: String,
    pub threshold: Option<Severity>,
}

impl AuditResult {
    pub fn blocked(&self) -> bool {
        self.threshold
            .is_some_and(|t| self.findings.iter().any(|f| f.severity >= t))
    }

    /// Byte-identical to v0.4.1 `AuditResult.result_hash` (sorted-key compact
    /// JSON list of findings, `"sha256:" + hex`).
    pub fn result_hash(&self) -> String {
        formatting::result_hash(&self.findings)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditPolicy {
    pub enabled: bool,
    pub profile: String,
    pub threshold: Option<Severity>,
    pub disabled_rules: Vec<String>,
}

impl Default for AuditPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            profile: "default".into(),
            threshold: None,
            disabled_rules: Vec::new(),
        }
    }
}

impl AuditPolicy {
    pub fn effective_threshold(&self) -> Option<Severity> {
        self.threshold.or(match self.profile.as_str() {
            "strict" => Some(Severity::High),
            "permissive" => None,
            _ => Some(Severity::Critical),
        })
    }

    fn is_disabled(&self, id: &str) -> bool {
        self.disabled_rules.iter().any(|rule| rule == id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditGateResult {
    pub results: Vec<AuditResult>,
    pub skipped: bool,
}

impl AuditGateResult {
    pub fn blocked(&self) -> bool {
        self.results.iter().any(AuditResult::blocked)
    }
}

/// v0.4.1 `policy_from_config`, including the profile's effective threshold.
pub fn policy_from_settings(settings: &AuditSettings) -> Result<AuditPolicy> {
    let mut disabled_rules = settings.disabled_rules.clone();
    disabled_rules.sort_unstable();
    disabled_rules.dedup();
    Ok(AuditPolicy {
        enabled: settings.enabled,
        profile: settings.profile.clone(),
        threshold: settings
            .threshold
            .as_deref()
            .map(Severity::parse)
            .transpose()?,
        disabled_rules,
    })
}

/// v0.4.1 `run_audit` for one skill directory.
pub fn run_audit(skill_dir: &Path, policy: &AuditPolicy) -> Result<AuditResult> {
    // pathlib removes redundant separators and "." components without
    // resolving ".." or links.
    let mut root: PathBuf = skill_dir
        .components()
        .filter(|component| *component != Component::CurDir)
        .collect();
    if root.as_os_str().is_empty() {
        root.push(".");
    }
    let skill = match root.components().next_back() {
        Some(Component::Normal(name)) => name.to_string_lossy().into_owned(),
        Some(Component::ParentDir) => "..".into(),
        _ => String::new(),
    };
    let mut result = AuditResult {
        skill,
        root,
        findings: Vec::new(),
        profile: policy.profile.clone(),
        threshold: None,
    };
    if policy.enabled {
        result.findings = analyzers::scan(&result.root, policy);
        result.threshold = policy.effective_threshold();
    }
    Ok(result)
}

/// v0.4.1 `audit_skills`: skipped when `skip` or the policy is disabled.
pub fn audit_skills(
    skill_dirs: &[PathBuf],
    policy: &AuditPolicy,
    skip: bool,
) -> Result<AuditGateResult> {
    if skip || !policy.enabled {
        return Ok(AuditGateResult {
            results: Vec::new(),
            skipped: true,
        });
    }
    let results = skill_dirs
        .iter()
        .map(|dir| run_audit(dir, policy))
        .collect::<Result<Vec<_>>>()?;
    Ok(AuditGateResult {
        results,
        skipped: false,
    })
}
