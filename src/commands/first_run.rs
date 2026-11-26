//! First-run guidance: the two ways to get a registry on a new machine.
//!
//! `NEXT_STEPS` is the single source of the text. `scripts/install.sh` and
//! `scripts/install.ps1` carry a copy; a test keeps them equal.

use std::process::ExitCode;

/// Shown when no registry is set up. `"$HOME/..."` is literal so the shell
/// expands it (sh, zsh, PowerShell) and keeps spaces: Windows PowerShell 5.1
/// does not expand `~` for native commands. One command per line because
/// Windows PowerShell 5.1 has no `&&`.
pub const NEXT_STEPS: &str = r#"Next:
  New here?            my-skills init-registry
  Registry elsewhere?  git clone <your-private-repo> "$HOME/my-agent-skills"
                       cd "$HOME/my-agent-skills"
                       my-skills bootstrap"#;

/// `my-skills` with no command.
///
/// Prints the same help as `--help`, followed by `NEXT_STEPS` when no registry
/// is configured anywhere on this machine.
pub fn bare() -> ExitCode {
    use clap::CommandFactory;
    let mut command = crate::cli::Cli::command();
    if let Some(bin_name) = std::env::args_os().next().and_then(|arg0| {
        std::path::Path::new(&arg0)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
    }) {
        command = command.bin_name(bin_name);
    }
    if command.print_help().is_err() {
        return ExitCode::FAILURE;
    }
    if let Err(error) = crate::registry::root::resolve_root(None, false)
        && crate::registry::root::is_unconfigured(&error)
    {
        println!();
        println!("{NEXT_STEPS}");
    }
    ExitCode::SUCCESS
}
