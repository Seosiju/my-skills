use std::process::ExitCode;

use clap::Parser;

fn main() -> ExitCode {
    my_skills::cli::run(my_skills::cli::Cli::parse())
}
