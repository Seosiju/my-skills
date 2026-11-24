//! `diff`: read-only comparison of a canonical skill with its host
//! connections. Wave 14 stream B.

use std::process::ExitCode;

use crate::cli::DiffArgs;

pub fn run(_args: &DiffArgs) -> ExitCode {
    super::not_implemented("diff")
}
