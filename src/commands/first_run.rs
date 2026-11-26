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
/// Contract stub: prints the same help as `--help`. Stream B adds the
/// first-run steps when no registry resolves.
pub fn bare() -> ExitCode {
    use clap::CommandFactory;
    let mut command = crate::cli::Cli::command();
    if command.print_long_help().is_err() {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
