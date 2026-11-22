//! Registry git hook commands.

use std::process::ExitCode;

use crate::cli::{HooksAction, HooksArgs, HooksRunArgs, SyncArgs};
use crate::{hooks, runtime};

pub fn run(args: &HooksArgs) -> ExitCode {
    if let HooksAction::Run(run) = &args.action {
        return run_hook(run);
    }
    let resolution = match runtime::resolve_root(None, false) {
        Ok(resolution) => resolution,
        Err(error) => return super::config_error(error),
    };
    let result = hooks::Git::discover(&resolution.root).and_then(|git| match &args.action {
        HooksAction::Install(args) => hooks::install(&git, args.dry_run),
        HooksAction::Uninstall(args) => hooks::uninstall(&git, args.dry_run),
        HooksAction::Status => hooks::status(&git).map(|status| {
            println!("Hooks:");
            status.print();
        }),
        HooksAction::Run(_) => unreachable!(),
    });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run_hook(args: &HooksRunArgs) -> ExitCode {
    match args.hook.as_str() {
        "pre-commit" => pre_commit(),
        "post-checkout" if args.git_args.get(2).map(String::as_str) != Some("1") => {
            ExitCode::SUCCESS
        }
        "post-merge" | "post-rewrite" | "post-checkout" => {
            let result = super::connect::sync(&SyncArgs {
                yes: true,
                quiet: true,
                ..Default::default()
            });
            if result != ExitCode::SUCCESS {
                eprintln!("warning: {} could not sync the registry", args.hook);
            }
            ExitCode::SUCCESS
        }
        hook => super::config_error(format!("unknown hook: {hook}")),
    }
}

fn pre_commit() -> ExitCode {
    let result = (|| {
        let root = match runtime::resolve_root(None, false) {
            Ok(resolution) => resolution.root,
            Err(error) => hooks::registry_with_index_manifest().ok_or(error)?,
        };
        hooks::pre_commit(&hooks::Git::discover(&root)?)
    })();
    match result {
        Ok(failures) if failures.is_empty() => ExitCode::SUCCESS,
        Ok(failures) => {
            for (skill, error) in failures {
                for line in error.lines() {
                    eprintln!("[BLOCKED] {skill}: {line}");
                }
            }
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("[BLOCKED] pre-commit: {error:#}");
            ExitCode::FAILURE
        }
    }
}
