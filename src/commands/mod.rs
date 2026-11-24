//! Command handlers. Each submodule is owned by one stream; `cli::run`
//! dispatches to them. Handlers print their own output and return the exit
//! code. Exit codes follow v0.4.1: 0 ok, 1 blocked/failed check, 2 usage or
//! configuration error.

use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};

pub mod connect; // integrator: install, sync, uninstall, convert, bootstrap
pub mod diff; // wave14 stream B: read-only canonical vs host comparison
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

/// Print a dry-run preview line when a real run would record the resolved
/// current-directory registry as the active registry cache.
pub fn print_would_cache_root(resolution: &crate::registry::root::RootResolution) {
    if resolution.would_cache() {
        println!(
            "  would remember {} as the active registry",
            crate::platform::paths::display_path(&resolution.root)
        );
    }
}

static JSON_ERRORS: AtomicBool = AtomicBool::new(false);

/// Record whether this invocation asked for `--json`. `cli::run` calls it once
/// before dispatching; wave 14 stream B makes `config_error` honour it.
pub fn set_json_errors(on: bool) {
    JSON_ERRORS.store(on, Ordering::Relaxed);
}

/// True when errors should also be printed as JSON on stdout.
pub fn json_errors() -> bool {
    JSON_ERRORS.load(Ordering::Relaxed)
}

/// Wave 14 contract shim: one optional skill and no `--group` until the
/// selection stream (A) implements several skills and groups.
pub(crate) fn one_skill<'a>(
    skills: &'a [String],
    groups: &[String],
) -> std::result::Result<Option<&'a str>, String> {
    if skills.len() > 1 || !groups.is_empty() {
        return Err("several skills and --group are not implemented yet".into());
    }
    Ok(skills.first().map(String::as_str))
}
