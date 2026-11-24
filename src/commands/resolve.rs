//! `resolve <name-or-path> [--json]` (wave15 S2). Read-only, hash-free lookup
//! from a skill name, a canonical path, or a host connection path (including
//! files inside the skill) to the canonical skill that owns it.
//!
//! Exit codes: 0 resolved (also `state: "unregistered"` for a draft directory
//! under the registry's skills/), 1 unresolved or unsafe path, 2 unknown skill
//! name, usage, or configuration error.

use std::process::ExitCode;

use crate::cli::ResolveArgs;

pub fn run(_args: &ResolveArgs) -> ExitCode {
    super::not_implemented("resolve")
}
