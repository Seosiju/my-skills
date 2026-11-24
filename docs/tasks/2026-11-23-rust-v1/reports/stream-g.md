# Stream G report

## Done

- Implemented `hooks install [--dry-run]`, `hooks uninstall`, and `hooks status`. Installation honors `core.hooksPath`, preflights all four hook files before writing, and prints the manual command for each foreign hook. Managed scripts use POSIX sh with LF endings, the v1 marker on line two, and mode 0755 on Unix. They look up `my-skills` on PATH and fall back to the quoted absolute executable path captured at install time.
- Pre-commit validates changed skill directories, supporting files, and a changed manifest from the index Git selected for the commit. It runs standard validation, the security audit, and applicable built-in host checks. Each failure identifies its skill or manifest.
- Post-merge, post-rewrite, and branch post-checkout call the integrator's sync handler with `yes` and `quiet` enabled. Failed sync returns a warning to Git and the hook exits 0. File checkout skips sync.
- Doctor reports the registry source, manifest health, hosts, temporary link probe, Git health, hooks, connection counts, and full validation. Its exit code is 1 for an invalid manifest and 0 for warnings, validation findings, or an unconfigured registry.

### Doctor JSON

| Key | Contents |
| --- | --- |
| `version`, `update`, `os`, `shell`, `python` | Text values from the corresponding environment lines. |
| `registry` | `path`, `source` (`env`, `cwd`, or `cache`), `error`, `warning`. |
| `skills` | `registered` and `enabled` counts, or null when the manifest did not load. |
| `state_path`, `data_root` | Local storage paths. |
| `hosts` | One entry per built-in host with `name`, `display_name`, `executable`, `enabled`, `writable`, and `path`. An undetected executable is null. |
| `manifest` | `status` of `valid`, `not_checked`, or `invalid`. An invalid manifest also has `error`. |
| `link_capability` | `kind` of `symlink`, `junction`, or `copy fallback`, with a `reason` for fallback. |
| `registry_git` | `branch`, `default_branch`, `rebase_in_progress`, `merge_in_progress`, `uncommitted`, `conflicts`, `upstream`, `ahead`, `behind`, and `warnings`. Unknown counts and refs are null. The section is null if no registry is available. |
| `hooks`, `hooks_error` | Hook directory `path`, `on_path` for the CLI, and `entries` with `name`, `status`, and `version`. A hook inspection error is reported in `hooks_error`. |
| `connections` | State record counts `link`, `copy`, `fallback`, plus `statuses` and `warnings`. Status counts cover registered skills against enabled targets and include only observed statuses. |
| `full_validation` | `checked` counts enabled skills. `failures` lists `skill` and `errors` for each failing skill. The section is null if the manifest did not load. |

## Tests

- `cargo build --locked` passed.
- `cargo test --locked` passed all 94 Rust tests, including 14 hook tests and 11 doctor tests. The tests used temporary HOME and XDG directories and synthetic registries.
- Direct `rustfmt --edition 2024 --check` passed for all tracked crate sources and the new tests. Direct `/opt/homebrew/bin/cargo-clippy clippy --all-targets -- -D warnings` and `git diff --check` passed.
- `cargo check --locked --all-targets --target x86_64-pc-windows-msvc` passed using Rust 1.100.0 and a Windows standard library stored under the worktree's ignored `target/` directory.
- `uv run --frozen python` generated v0.4.1 doctor output in temporary environments for cached, unconfigured, and invalid registries, using a stubbed update lookup. The v0.4.1 doctor tests and `tests/test_cli.py::test_doctor_exit_zero` passed all 10 cases.

## Differences from v0.4.1

- Deliberate. Hooks, doctor JSON, and the doctor sections after manifest health are new in v1. The version is the Rust CLI version, `OS:` renders the compiled OS and architecture, and `Python:` reads `not required (Rust binary)`. Manifest parse details come from the shared Rust TOML loader.
- Deliberate. Doctor retains the v0.4.1 `Update:  skipped` line for `--no-update-check`. Other update text comes from Stream I's update status provider.
- Deliberate implementation choice. Pre-commit exports the blobs named by `git ls-files --stage -z` using `git cat-file --batch`. A normal `checkout-index` export can rewrite staged content with a smudge filter, including a filter selected by an unstaged `.gitattributes` file. The snapshot uses the staged shared manifest without a working-tree local override. Nonregular index entries in an exported skill are blocked so validation stays within the staged snapshot. When an unchanged manifest cannot be loaded, pre-commit uses the built-in skill root and host defaults, and doctor reports the invalid working manifest.

## Contract change requests

- Add a dry-run flag to the `HooksAction::Uninstall` CLI variant to satisfy the spec's general dry-run rule for writes. The phase-2 skeleton exposes that flag on `hooks install` only.

## Open issues

- The sync handler and default update lookup are placeholders on the phase-2 base. Post-hook exit behavior and doctor skip behavior are verified. A successful Git-triggered reconciliation and live update status through doctor depend on the integrator and Stream I implementations.
- The MSVC check type-checked library, binary, and tests. Git Bash hook execution and the actual junction probe still need Windows runtime verification.
