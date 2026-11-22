//! `hooks install|uninstall|status|run`. Stream G. Spec §6.7.

use std::process::ExitCode;

use crate::cli::*;

pub fn run(_args: &HooksArgs) -> ExitCode {
    super::not_implemented(module_path!().rsplit("::").next().unwrap_or("command"))
}
