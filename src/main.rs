use std::process::ExitCode;

use clap::{Parser, error::ErrorKind};
use my_skills::{cli, commands};

fn main() -> ExitCode {
    match cli::Cli::try_parse() {
        Ok(cli) => cli::run(cli),
        Err(error) => {
            if !matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) {
                let raw_args: Vec<_> = std::env::args_os().collect();
                let wants_json = raw_args.iter().any(|arg| arg == "--json");
                if let Some((message, hint)) = cli::removed_command_hint(&raw_args) {
                    eprintln!("error: {message}");
                    if wants_json {
                        commands::print_json_error("usage", message, Some(hint));
                    }
                    return ExitCode::from(2);
                }
                if wants_json {
                    commands::print_json_error("usage", error.to_string().trim_end(), None);
                }
            }
            error.exit()
        }
    }
}
