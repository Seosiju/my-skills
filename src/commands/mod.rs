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
pub mod resolve; // wave15 S2: read-only name/path -> canonical lookup
pub mod update; // stream I

/// Placeholder until a command is wired; keeps alpha builds honest.
pub fn not_implemented(name: &str) -> ExitCode {
    eprintln!("my-skills {name}: not implemented yet in this v1 alpha build");
    ExitCode::from(2)
}

/// Print a configuration error to stderr and, for JSON invocations, stdout.
pub fn config_error(err: impl std::fmt::Display) -> ExitCode {
    let message = err.to_string();
    eprintln!("error: {message}");
    if json_errors() {
        let (code, hint) = config_error_details(&message);
        print_json_error(code, &message, hint);
    }
    ExitCode::from(2)
}

/// Every "state lock timed out" message starts with this; `config_error` maps
/// it to the `state_busy` JSON code (exit 2). Wave 15 S1 produces it.
pub const STATE_BUSY_PREFIX: &str = "state is busy: ";

/// Stable codes for errors currently passed through the display-only handler.
/// Match the originating diagnostics here so existing human output stays intact.
fn config_error_details(message: &str) -> (&'static str, Option<&'static str>) {
    match message {
        "install --json requires --dry-run"
        | "sync --json requires --dry-run"
        | "sync --json is not supported with --check; use --dry-run --json" => {
            ("usage", Some("Use --dry-run --json to request a plan."))
        }
        "share requires a skill name unless --plan is used"
        | "share requires exactly one of --enable or --disable"
        | "uninstall requires a skill name"
        | "enable requires a skill name"
        | "disable requires a skill name"
        | crate::cli::ROOT_WITH_SET_ROOT => ("usage", None),
        m if m.starts_with("--root ") && m.ends_with(" does not contain my-skills.toml") => (
            "usage",
            Some("Pass --root a registry folder that contains my-skills.toml."),
        ),
        m if m.starts_with("refusing to write to cwd registry ") => (
            "inactive_registry",
            Some(
                "Run from the active registry, pass --root <path>, or switch with my-skills set-root.",
            ),
        ),
        m if m.starts_with("unknown skill: ") || m.starts_with("unknown skill(s): ") => {
            ("unknown_skill", None)
        }
        m if m.starts_with("unknown group: ") => ("unknown_group", None),
        m if m.starts_with("unknown host: ") => ("unknown_host", None),
        m if m.starts_with("skill already exists: ") => (
            "already_exists",
            Some("Pick another name, or edit the existing skill (my-skills resolve <name>)."),
        ),
        m if m.starts_with("invalid skill name ") || m.starts_with("invalid group name ") => (
            "invalid_name",
            Some("Use lowercase letters, digits, and single hyphens."),
        ),
        m if m.starts_with(STATE_BUSY_PREFIX) => (
            "state_busy",
            Some("Another my-skills command is writing; retry when it finishes."),
        ),
        _ => ("config", None),
    }
}

/// Emit one JSON error record on stdout, including before CLI dispatch.
pub fn print_json_error(code: &str, message: &str, hint: Option<&str>) {
    println!(
        "{}",
        serde_json::json!({"error": {"code": code, "message": message, "hint": hint}})
    );
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

/// Record whether this invocation asked for JSON output before dispatch.
pub fn set_json_errors(on: bool) {
    JSON_ERRORS.store(on, Ordering::Relaxed);
}

/// True when errors should also be printed as JSON on stdout.
pub fn json_errors() -> bool {
    JSON_ERRORS.load(Ordering::Relaxed)
}
