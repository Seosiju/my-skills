//! init-registry, skills, share, enable, disable, import. Stream H.

use std::process::ExitCode;

use crate::cli::*;

pub fn init_registry(_args: &InitRegistryArgs) -> ExitCode {
    super::not_implemented("init-registry")
}

pub fn skills(_args: &SkillsArgs) -> ExitCode {
    super::not_implemented("skills")
}

pub fn share(_args: &ShareArgs) -> ExitCode {
    super::not_implemented("share")
}

pub fn enable(_args: &SkillNameArgs) -> ExitCode {
    super::not_implemented("enable")
}

pub fn disable(_args: &SkillNameArgs) -> ExitCode {
    super::not_implemented("disable")
}

pub fn import(_args: &ImportArgs) -> ExitCode {
    super::not_implemented("import")
}
