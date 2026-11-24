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
            ) && std::env::args_os().any(|arg| arg == "--json")
            {
                commands::print_json_error("usage", error.to_string().trim_end(), None);
            }
            error.exit()
        }
    }
}
