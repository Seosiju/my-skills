//! Command handlers. Each submodule is owned by one stream; `cli::run`
//! dispatches to them. Handlers print their own output and return the exit
//! code. Exit codes follow v0.4.1: 0 ok, 1 blocked/failed check, 2 usage or
//! configuration error.

use std::process::ExitCode;

pub mod connect; // integrator: install, sync, uninstall, convert, bootstrap
pub mod doctor; // stream G
pub mod hooks; // stream G
pub mod inspect; // integrator: validate, audit, status, set-root, data-path
pub mod registry; // stream H: init-registry, skills, share, enable, disable, import
pub mod update; // stream I

/// Placeholder until a command is wired; keeps alpha builds honest.
pub fn not_implemented(name: &str) -> ExitCode {
    eprintln!("my-skills {name}: not implemented yet in this v1 alpha build");
    ExitCode::from(2)
}

/// Print `error: <err>` to stderr and return exit code 2, as v0.4.1 does for
/// manifest and configuration errors.
pub fn config_error(err: impl std::fmt::Display) -> ExitCode {
    eprintln!("error: {err}");
    ExitCode::from(2)
}
