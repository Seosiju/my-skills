//! Agent Skills standard validation. Stream D. Mirrors v0.4.1
//! `validation.py`. (v0.4.1 `checks.compose_validation`, which adds audit
//! findings, is wired up by the integrator once stream E lands.)

use std::path::{Path, PathBuf};

use crate::hosts::HostConfig;

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
pub fn validate_name(_name: &str) -> Vec<String> {
    todo!("stream D")
}

/// Same checks and message text as v0.4.1 `validate_skill`.
pub fn validate_skill(_path: &Path) -> ValidationResult {
    todo!("stream D")
}

/// Same checks and message text as v0.4.1 `validate_skill_for_host`.
pub fn validate_skill_for_host(_path: &Path, _host: &HostConfig) -> ValidationResult {
    todo!("stream D")
}
