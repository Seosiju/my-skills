# my-skills Architecture

`my-skills` is a single Rust binary (`src/main.rs` → `src/cli.rs`) built around a clean split between the **canonical side** (`registry/`, reading and editing your skill repo) and the **connection side** (`drive/`, planning and applying links or copies into agent host folders). Command handlers in `commands/` only coordinate flow, print output, and pick exit codes (`0` ok, `1` blocked or failed check, `2` usage or configuration error). All shared checks live in `check/`, Git hook management lives in `git/`, OS path lookup lives in `platform/`, and self-update lives in `update.rs`.

A Korean walkthrough with diagrams of the whole system (glossary, disk layout, connection states, edit/commit, install/sync, `new`, `resolve`, module map, and safety checks) is in [system-map.ko.md](system-map.ko.md).

## Overview

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="images/architecture-dark.png">
  <img alt="my-skills internal architecture: cli.rs dispatches to commands/ (resolve.rs and command handlers), which coordinate src/registry/, src/drive/ (hosts/plan, staged copy snapshots in apply/link, state.rs, and drive/lock.rs for state.json.lock), src/check/, src/git/hooks.rs (5 hooks including pre-commit and post-commit), src/platform/, and src/update.rs to read and write the active root cache, registry files, host skill folders, install state and lock, .git/hooks, machine-local data, and GitHub Releases." src="images/architecture.png">
</picture>

## Modules

| Path | Owns | Main entry points |
| --- | --- | --- |
| `src/cli.rs` | `clap` CLI definitions, argument structs, and subcommand dispatch. | `Cli`, `Command`, `run` |
| `src/commands/` | Per-command orchestration and output (`connect`, `inspect`, `registry`, `doctor`, `hooks`, `update`, `diff`). | `connect::{install, sync, uninstall, convert, bootstrap}`, `inspect::*`, `registry::*`, `doctor::run`, `hooks::run`, `update::run` |
| `src/commands/diff.rs` | Resolves a skill, uses `plan::status_of` for host status, and reports the live comparison without writing. | `diff::run` |
| `src/commands/resolve.rs` | Read-only, hash-free lookup from a skill name, canonical path, or host path (including files inside a skill) to the canonical file, with one `state` and exit `0`/`1`/`2`. | `resolve::run` |
| `src/registry/` | Canonical repo side: root lookup, manifest + local overlay loading, comment-preserving edits, catalog output, seed scaffolding, and `import`/`share`. | `root::load_manifest_resolved`, `manifest::load_manifest`, `edit::prepare_*`, `catalog::rows`, `seeds::Scaffold`, `share::{import, share, plan_share}` |
| `src/registry/manifest.rs` | Resolves skill names and repeatable group selections to a sorted, unique set of registered names. | `manifest::select_names` |
| `src/drive/` | Host connection side: built-in host definitions, read-only planning, atomic apply/rollback, symlink/junction handling, deterministic directory hashing, ignore rules, and `state.json`. | `hosts::ALL`, `plan::{plan_install_one, plan_sync, plan_convert, status_of}`, `apply::apply`, `link::{entry_kind, create_link, remove_link}`, `hash::hash_directory`, `state::State`, `lock::StateLock` |
| `src/drive/diff.rs` | Compares directory inventories with the drive's ignore rules and file hashing, and builds optional unified patches. | `diff::compare` |
| `src/check/` | Agent Skills format and host display diagnostics. | `frontmatter::parse_document`, `validate::{validate_skill, validate_skill_for_host, gate_errors}` |
| `src/git/` | Managed Git hook installation, status checks, and staged-index `pre-commit` validation. | `hooks::{Git, install, uninstall, status, pre_commit}` |
| `src/platform/` | Cross-platform home/config/state/data directories and Windows verbatim-path display helpers. | `paths::{home_dir, config_home, state_home, data_root, display_path}`, `data::skill_data_path` |
| `src/update.rs` | GitHub Releases lookup, `SHA256SUMS` verification, extraction, executable lock, and rollback-safe swap. | `latest_release`, `UpdateDownload::{new, install}` |
| `src/error.rs` | Crate-wide `Error` enum and `Result` alias. | `Error`, `Result` |

## Main flows

### `install` and `sync` (`src/commands/connect.rs`)
1. **Resolve root and load manifest.** `registry::root::load_manifest_resolved` checks `--root`, `MY_SKILLS_ROOT`, `my-skills.toml` in cwd or its ancestors, then the saved active root. `--root`, read-only commands, and previews leave the cache alone. When lookup selects cwd, a real write saves a first discovery or refuses with exit `2` if another active root exists. `RegistryMeta` exposes `{path, source}` for JSON output. `registry::manifest::load_manifest` merges the manifest, local overlay, and CLI overrides.
2. **Lock, load state & plan**: a real write first takes the exclusive `state.json.lock` (`drive::lock::StateLock`, once per command, after the manifest edit lock when there is one; up to 30 s, then `state_busy` exit `2`). Previews and `sync --check` do not lock. `drive::state::State::load` reads `~/.local/state/my-skills/state.json`. `InstallRecord::belongs_to` (`src/drive/state.rs`) verifies registry ownership by comparing the canonicalized parent of `record.source` against `manifest.skills_dir()`, and `src/drive/plan.rs` marks other registries' connections `FOREIGN` / `BLOCK_FOREIGN` (silently skipping them when `plan_sync` prunes). `plan_install_one` and `plan_sync` inspect host paths via `drive::link::entry_kind` and hash copies via `drive::hash::hash_directory` without writing anything.
3. **Validate**. `check::validate::{validate_skill, validate_skill_for_host}` checks that `SKILL.md` exists and reports format and host display warnings. Copy connections are checked on one staged snapshot, and that snapshot is what gets installed and hashed. Previews run the same checks. Explicit `sync <SKILL>`/`--group` checks every selected skill that stays connected, including FRESH links; bare `sync` checks planned creates/updates. `plan::group_destinations` blocks a single-host change to an entry an unselected record shares (`SHARED_DESTINATION`).
4. **Apply per item with rollback**: `drive::apply::apply` creates links (`drive::link::create_link`) or materializes the snapshot into sibling temp dirs and swaps them into place, re-hashing the host copy just before the swap (a late host edit becomes `CONFLICT`). After each item, it saves `state.json` atomically (`State::save` via `.tmp` + rename); if disk or state save fails, it restores the backup/previous entry and memory state. A backup that cannot be deleted after commit is a cleanup warning, not a failure.
5. **Report**: `commands/connect.rs` prints per-item outcomes and exits `1` if any item failed or was blocked (`DRIFTED`, `CONFLICT`, `UNMANAGED`, `BROKEN`, `FOREIGN`/`BLOCK_FOREIGN`). Dry runs emit a top-line `BLOCKED:` summary in text mode (plus a `--yes` hint when multi-host confirmation is needed) or UTF-8 JSON with `registry`, `dry_run`, `blocked`, `applicable`, `block_reasons`, `validation`, and `actions`, exiting `1` only on validation failure. Without `--dry-run`, `--json` applies the plan and prints one document that adds `applied`, `summary`, `cleanup_warnings`, and each action's `result`.

### `import` and `share` (`src/commands/registry.rs` → `src/registry/share.rs`)
1. **Stage safely**: `Snapshot::new` opens the source with no-follow directory handles (`cap_std`), rejects symlinks anywhere in the tree, and copies into a staging tempdir.
2. **Validate**. Runs `check::validate::validate_skill` against the staged snapshot and compares hashes against any existing `skills/<name>/`.
3. **Prepare manifest edit**: `registry::edit::prepare_import` or `prepare_registration` acquires `my-skills.toml.edit-lock` and builds a comment-preserving `ManifestEdit`.
4. **Publish & commit**: `Snapshot::publish` verifies the destination does not overlap any host directory, swaps the snapshot into `skills/<name>/` (backing up any overwritten version), commits `my-skills.toml` atomically, and for `share` saves the adopted `InstallRecord` to `state.json`. Any failure rolls back both `skills/<name>/` and `my-skills.toml`.

### Git hooks (`src/commands/hooks.rs` → `src/git/hooks.rs`)
1. `my-skills hooks install` (and `hooks uninstall`) follow the real-write root policy (refusing a non-active cwd registry and caching a first-ever active root unless `--dry-run`) and write five shell wrappers (`pre-commit`, `post-commit`, `post-merge`, `post-rewrite`, `post-checkout`) into `.git/hooks/`, rewriting managed ones and leaving foreign hooks in place (exit `1` while a foreign hook remains). Each wrapper exports `MY_SKILLS_ROOT` for that registry and invokes `my-skills hooks run <name> "$@"`.
1a. **Automatic install (`hooks::fill`)**: `init-registry`, `bootstrap`, and every `hooks run <name>` write only the *missing* wrappers. Managed and foreign hooks are never rewritten. `fill` writes only when the registry is the Git toplevel (empty `--show-prefix`), not a linked worktree, `core.hooksPath` is unset, and `.git/hooks` is not a symlink or junction. Otherwise setup commands print `Git hooks: skipped (<reason>)` and hook runs stay quiet. In a hook run, a fill failure is a warning and never changes the hook's exit code. `--no-hooks` turns off the setup step.
2. **`pre-commit`**: `hooks::pre_commit` reads staged files (`git diff --cached`), exports exact staged blobs (`git cat-file --batch`) into a temp snapshot without checkout filters, and runs `check::validate::gate_errors` on each changed skill plus manifest loading if `my-skills.toml` changed.
3. **`post-commit`**: `commands/hooks.rs` takes the state lock, then `hooks::post_commit` decides what to sync from `HEAD` (`diff-tree --root -m --first-parent`): the changed registered skills, or everything when `my-skills.toml` changed. Dirty skills are excluded (never connected or pruned); an uncommitted `my-skills.toml` skips the hook. It runs `connect::sync_locked` with `--yes --quiet` and always exits `0`.
4. **`post-merge`, `post-rewrite`, `post-checkout`** (branch switches only, flag `1`): `commands/hooks.rs` runs `commands::connect::sync` with `--yes --quiet`, printing a warning instead of failing Git if sync reports problems (including when invoked without `MY_SKILLS_ROOT` inside a non-active clone).
5. Every post-* hook is a no-op in a linked worktree (detected from Git metadata), and post-commit also while a rebase, merge, or cherry-pick is in progress.

### `update` (`src/commands/update.rs` → `src/update.rs`)
1. `latest_release` fetches the GitHub Releases API (`stable` picks highest non-prerelease `v1.*`; `main` picks highest semver tag) and compares against `CARGO_PKG_VERSION`.
2. `UpdateDownload::install` downloads `SHA256SUMS` and the platform archive, checks the SHA-256 digest, and extracts the binary into a temp directory beside the current executable.
3. `replace` locks `<exe>.update-lock`, confirms the installed binary still matches `CURRENT_VERSION`, backs up the current executable, renames the staged binary into place, and verifies `--version` — restoring the backup if anything fails.

## Where to start

- **Add a built-in host**: add a `HostConfig` constant and register it in `ALL` in `src/drive/hosts.rs`, update the default manifest template and `SEED_HOSTS` in `src/registry/seeds.rs`, and add host-specific diagnostics in `src/commands/doctor.rs` if needed.
- **Add a CLI command**: add the subcommand and args struct in `src/cli.rs`, wire `cli::run` to a handler in `src/commands/` (keeping business logic in `registry/`, `drive/`, `check/`, or `git/`), and add integration tests under `tests/`.
