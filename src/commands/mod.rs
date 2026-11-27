//! Command handlers. `cli::run` dispatches to them. Handlers print their own
//! output and return the exit code: 0 ok, 1 blocked/failed check, 2 usage or
//! configuration error.

use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};

pub mod connect; // sync, bootstrap
pub mod diff; // read-only canonical vs host comparison
pub mod doctor; // health report
pub mod first_run; // first-run guidance
pub mod hooks; // git hooks install/uninstall/status/run
pub mod inspect; // validate, set-root, data-path
pub mod registry; // init-registry, skills, enable, disable, import, new
pub mod update; // self-update

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
/// it to the `state_busy` JSON code (exit 2).
pub const STATE_BUSY_PREFIX: &str = "state is busy: ";

/// Stable codes for errors currently passed through the display-only handler.
/// Match the originating diagnostics here so existing human output stays intact.
fn config_error_details(message: &str) -> (&'static str, Option<&'static str>) {
    match message {
        "sync --json is not supported with --check; use --dry-run --json" => {
            ("usage", Some("Use --dry-run --json to request a plan."))
        }
        "import requires a source path unless --from is used"
        | "--plan requires --from <host>"
        | "--plan cannot be combined with a skill name"
        | "import --from requires a skill name unless --plan is used"
        | "import --from requires either --enable or --disable"
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
        m if m.starts_with(crate::commands::registry::NEW_DESCRIPTION_ERROR) => (
            "usage",
            Some("Pass a non-empty --description within the hosts' character limit."),
        ),
        m if m.starts_with("skill already exists: ") => (
            "already_exists",
            Some("Pick another name, or edit the existing skill (my-skills skills <name>)."),
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
