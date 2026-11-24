//! `update`. Stream I. Spec §6.9.

use std::cmp::Ordering;
use std::process::ExitCode;

use crate::cli::{Channel, UpdateArgs};
use crate::platform::paths;
use crate::update::{CURRENT_VERSION, UpdateDownload, latest_release};

pub fn run(args: &UpdateArgs) -> ExitCode {
    let channel = match args.channel {
        Channel::Stable => "stable",
        Channel::Main => "main",
    };
    println!("Current: my-skills {CURRENT_VERSION}");
    println!("Channel: {channel}");

    let (comparison, release) = match latest_release(args.channel)
        .and_then(|release| Ok((release.compare_current()?, release)))
    {
        Ok(target) => target,
        Err(error) if args.check => {
            eprintln!("Update status unknown: {error:#}");
            return ExitCode::from(2);
        }
        Err(error) => {
            return super::config_error(format_args!(
                "could not resolve update target ({error:#})"
            ));
        }
    };

    println!("Latest:  {}", release.tag);
    match comparison {
        Ordering::Equal => {
            println!("Already up to date");
            return ExitCode::SUCCESS;
        }
        Ordering::Greater => {
            println!("Installed version is ahead of the latest {channel} release");
            return ExitCode::SUCCESS;
        }
        Ordering::Less if args.check => {
            println!("Update available");
            return ExitCode::from(1);
        }
        Ordering::Less => {}
    }

    let download = match UpdateDownload::new(release) {
        Ok(download) => download,
        Err(error) => return super::config_error(error),
    };
    if args.dry_run {
        println!("Would download: {}", download.checksums_url);
        println!("Would download: {}", download.archive_url);
        println!(
            "Would replace:  {}",
            paths::display_path(&download.executable)
        );
        return ExitCode::SUCCESS;
    }

    println!("Downloading: {}", download.archive_url);
    match download.install() {
        Ok(version) => {
            let suffix = if args.channel == Channel::Main {
                " from main"
            } else {
                ""
            };
            println!("Updated: {version}{suffix}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::from(1)
        }
    }
}
