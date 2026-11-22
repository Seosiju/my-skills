//! Skill security audit. Stream E. Mirrors v0.4.1 `audit/` including
//! `builtin_rules.yaml`. Stream E may add private submodules here
//! (analyzers, rules, markdown, dataflow, ...).

use std::path::{Path, PathBuf};

use crate::Result;
use crate::manifest::AuditSettings;

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
        todo!("stream E")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditPolicy {
    pub enabled: bool,
    pub profile: String,
    pub threshold: Option<Severity>,
    pub disabled_rules: Vec<String>,
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
pub fn policy_from_settings(_settings: &AuditSettings) -> Result<AuditPolicy> {
    todo!("stream E")
}

/// v0.4.1 `run_audit` for one skill directory.
pub fn run_audit(_skill_dir: &Path, _policy: &AuditPolicy) -> Result<AuditResult> {
    todo!("stream E")
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
