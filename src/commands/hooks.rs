//! Registry git hook commands.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::ExitCode;

use crate::cli::{HooksAction, HooksArgs, HooksRunArgs, SyncArgs};
use crate::drive::state::{State, StateLock, default_state_path};
use crate::registry::manifest::{self, CliOverrides};
use crate::{git::hooks, registry::root};

pub(super) const NOT_A_GIT_REPOSITORY: &str = "not a Git repository";

pub fn run(args: &HooksArgs) -> ExitCode {
    let write_cache = match &args.action {
        HooksAction::Install(args) => !args.dry_run,
        HooksAction::Uninstall(args) => !args.dry_run,
        HooksAction::Status => false,
        HooksAction::Run(run) => return run_hook(run),
    };
    let resolution = match root::resolve_root(None, write_cache) {
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
    let hook = args.hook.as_str();
    if !hooks::NAMES.contains(&hook) {
        return super::config_error(format!("unknown hook: {hook}"));
    }
    if let Ok(resolution) = root::resolve_root(None, false)
        && let Ok(git) = hooks::Git::discover(&resolution.root)
    {
        report_hook_fill(hooks::fill_from_hook(&git), |message| {
            eprintln!("{message}")
        });
    }
    match hook {
        "pre-commit" => pre_commit(),
        "post-checkout" if args.git_args.get(2).map(String::as_str) != Some("1") => {
            ExitCode::SUCCESS
        }
        "post-commit" => post_commit(),
        "post-merge" | "post-rewrite" | "post-checkout" => {
            if in_linked_worktree() {
                return ExitCode::SUCCESS;
            }
            // Sync owns the state lock. Post-operation hooks report failures
            // without changing Git's exit status.
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
        _ => unreachable!(),
    }
}

fn report_hook_fill(
    result: anyhow::Result<Option<hooks::FilledHooks>>,
    mut report: impl FnMut(String),
) {
    let error = match result {
        Ok(Some(filled)) => {
            for name in filled.installed {
                report(format!("my-skills: added missing hook {name}"));
            }
            filled.error
        }
        Ok(None) => None,
        Err(error) => Some(error),
    };
    if let Some(error) = error {
        report(format!(
            "warning: my-skills could not add missing hooks: {error:#}"
        ));
    }
}

pub(super) fn preview_setup(root: &Path) {
    let plan = match hooks::Git::discover(root) {
        Ok(git) => hooks::plan_fill(&git),
        Err(_) => Ok(hooks::FillPlan::Skipped(NOT_A_GIT_REPOSITORY)),
    };
    match plan {
        Ok(hooks::FillPlan::Skipped(reason)) => println!("  git hooks: skipped ({reason})"),
        Ok(hooks::FillPlan::Eligible(status)) => status.print_fill_preview(),
        Err(error) => eprintln!("warning: could not install git hooks: {error:#}"),
    }
}

pub(super) fn fill_for_setup(git: &hooks::Git<'_>) -> bool {
    match hooks::fill(git) {
        Ok(hooks::FillOutcome::Applied(hooks::FilledHooks {
            path,
            installed,
            foreign,
            error,
        })) => {
            for name in &installed {
                println!("installed: {}", hooks::display_hook_path(&path.join(name)));
            }
            for name in foreign {
                hooks::print_foreign(&path.join(name), name);
            }
            if let Some(error) = error {
                eprintln!("warning: could not install git hooks: {error:#}");
            }
            !installed.is_empty()
        }
        Ok(hooks::FillOutcome::Skipped(reason)) => {
            println!("Git hooks: skipped ({reason})");
            false
        }
        Err(error) => {
            eprintln!("warning: could not install git hooks: {error:#}");
            false
        }
    }
}

/// The installed hooks export a relative registry path, so inside a linked
/// worktree they would point at that worktree instead of the registry.
fn in_linked_worktree() -> bool {
    let Ok(resolution) = root::resolve_root(None, false) else {
        return false;
    };
    hooks::Git::discover(&resolution.root)
        .and_then(|git| git.in_linked_worktree())
        .unwrap_or(false)
}

/// Refresh hosts after a commit that passed pre-commit. The commit already
/// happened, so every outcome exits 0 and problems are warnings on stderr.
///
/// The state lock is taken before any git check and held through the sync, so
/// edits made while this hook waits for another writer are seen by the checks
/// rather than copied. A millisecond window remains between the last check and
/// the copy itself; the status re-check at the end reports such edits.
fn post_commit() -> ExitCode {
    let could_not_sync = || eprintln!("warning: post-commit could not sync the registry");
    let resolution = match root::resolve_root(None, false) {
        Ok(resolution) => resolution,
        Err(error) => {
            eprintln!("warning: post-commit could not inspect the commit: {error:#}");
            return ExitCode::SUCCESS;
        }
    };
    let git = match hooks::Git::discover(&resolution.root) {
        Ok(git) => git,
        Err(error) => {
            eprintln!("warning: post-commit could not inspect the commit: {error:#}");
            return ExitCode::SUCCESS;
        }
    };
    // A linked worktree never syncs, so it need not wait for the lock.
    if git.in_linked_worktree().unwrap_or(false) {
        return ExitCode::SUCCESS;
    }
    if let Some(refusal) = resolution.write_refusal() {
        eprintln!("error: {refusal}");
        could_not_sync();
        return ExitCode::SUCCESS;
    }
    let _state_lock = match StateLock::acquire(&default_state_path()) {
        Ok(lock) => lock,
        Err(error) => {
            eprintln!("error: {error}");
            could_not_sync();
            return ExitCode::SUCCESS;
        }
    };
    if let Err(error) = resolution.recheck_write() {
        eprintln!("error: {error}");
        could_not_sync();
        return ExitCode::SUCCESS;
    }
    let manifest = match manifest::load_manifest(&resolution.root, &CliOverrides::default()) {
        Ok(manifest) => manifest,
        Err(error) => {
            eprintln!("warning: post-commit could not inspect the commit: {error:#}");
            return ExitCode::SUCCESS;
        }
    };
    let recorded: BTreeSet<String> = State::load(None)
        .map(|state| state.records().map(|record| record.skill.clone()).collect())
        .unwrap_or_default();
    let plan = match hooks::post_commit(&git, &manifest, &recorded) {
        Ok(hooks::PostCommit::Sync(plan)) => plan,
        Ok(hooks::PostCommit::Skip(warning)) => {
            if let Some(warning) = warning {
                eprintln!("{warning}");
            }
            return ExitCode::SUCCESS;
        }
        Err(error) => {
            eprintln!("warning: post-commit could not inspect the commit: {error:#}");
            return ExitCode::SUCCESS;
        }
    };
    for skill in &plan.warn {
        eprintln!("warning: post-commit left {skill} alone: it has uncommitted changes");
    }
    if !plan.full && plan.skills.is_empty() {
        return ExitCode::SUCCESS;
    }
    let result = super::connect::sync_locked(
        &manifest,
        &SyncArgs {
            skills: plan.skills.clone(),
            yes: true,
            quiet: true,
            exclude: plan.exclude.clone(),
            ..Default::default()
        },
        &resolution,
    );
    if result != ExitCode::SUCCESS {
        could_not_sync();
    }
    // Edits made while the sync ran may have been copied along with the commit.
    if let Ok(dirty) = hooks::dirty_skills(&git, &manifest) {
        let synced = |name: &String| {
            !plan.exclude.contains(name)
                && (if plan.full {
                    manifest.skills.contains_key(name) || recorded.contains(name)
                } else {
                    plan.skills.contains(name)
                })
        };
        let changed: Vec<_> = dirty.iter().filter(|name| synced(name)).cloned().collect();
        if !changed.is_empty() {
            eprintln!(
                "warning: post-commit: {} changed while syncing; the host copies may include uncommitted edits (the next commit or sync refreshes them)",
                changed.join(", ")
            );
        }
    }
    ExitCode::SUCCESS
}

fn pre_commit() -> ExitCode {
    let result = (|| {
        let root = match root::resolve_root(None, false) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_fill_reports_successful_writes_before_the_warning() {
        let filled = hooks::FilledHooks {
            path: "hooks".into(),
            installed: vec!["pre-commit", "post-merge"],
            foreign: Vec::new(),
            error: Some(anyhow::anyhow!("write failed")),
        };
        let mut messages = Vec::new();
        report_hook_fill(Ok(Some(filled)), |message| messages.push(message));
        assert_eq!(
            messages,
            [
                "my-skills: added missing hook pre-commit",
                "my-skills: added missing hook post-merge",
                "warning: my-skills could not add missing hooks: write failed",
            ]
        );
    }
}
