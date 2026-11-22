//! validate, audit, status, set-root, data-path. Integrator.

use std::process::ExitCode;

use crate::cli::*;

pub fn validate(_args: &ValidateArgs) -> ExitCode {
    super::not_implemented("validate")
}

pub fn audit(_args: &AuditArgs) -> ExitCode {
    super::not_implemented("audit")
}

pub fn status(_args: &StatusArgs) -> ExitCode {
    super::not_implemented("status")
}

pub fn set_root(_args: &SetRootArgs) -> ExitCode {
    super::not_implemented("set-root")
}

pub fn data_path(_args: &DataPathArgs) -> ExitCode {
    super::not_implemented("data-path")
}
