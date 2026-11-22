//! `doctor`. Stream G. Spec §6.8.

use std::process::ExitCode;

use crate::cli::*;

pub fn run(_args: &DoctorArgs) -> ExitCode {
    super::not_implemented(module_path!().rsplit("::").next().unwrap_or("command"))
}
