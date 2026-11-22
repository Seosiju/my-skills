//! `update`. Stream I. Spec §6.9.

use std::process::ExitCode;

use crate::cli::*;

pub fn run(_args: &UpdateArgs) -> ExitCode {
    super::not_implemented(module_path!().rsplit("::").next().unwrap_or("command"))
}
