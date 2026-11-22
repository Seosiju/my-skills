//! install, sync, uninstall, convert, bootstrap. Integrator.

use std::process::ExitCode;

use crate::cli::*;

pub fn install(_args: &InstallArgs) -> ExitCode {
    super::not_implemented("install")
}

pub fn sync(_args: &SyncArgs) -> ExitCode {
    super::not_implemented("sync")
}

pub fn uninstall(_args: &UninstallArgs) -> ExitCode {
    super::not_implemented("uninstall")
}

pub fn convert(_args: &ConvertArgs) -> ExitCode {
    super::not_implemented("convert")
}

pub fn bootstrap(_args: &BootstrapArgs) -> ExitCode {
    super::not_implemented("bootstrap")
}
