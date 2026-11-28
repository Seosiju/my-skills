# Rust v1 phase 2: parallel streams

Phase 1 (`README.md`, streams A–F) is merged. Its rules 1–10 still apply. This
file adds what changed for phase 2.

| Stream | Brief | Branch | Worktree |
|---|---|---|---|
| G hooks + doctor | `stream-g-hooks-doctor.md` | `dev-stream-g` | `<worktrees>/stream-g` |
| H registry commands | `stream-h-registry.md` | `dev-stream-h` | `<worktrees>/stream-h` |
| I update | `stream-i-update.md` | `dev-stream-i` | `<worktrees>/stream-i` |
| integrator | install, sync, uninstall, convert, bootstrap, validate, audit, status, set-root, data-path | `dev` | main checkout |

## What is already there

- `crate::runtime` resolves the registry root and loads the manifest
  (`load_manifest_from_cwd`). It also selects skills and hosts and confirms
  multi-host writes. Use it; do not re-implement it.
- `crate::cli` defines every command's arguments to match v0.4.1. Your handler
  receives the parsed `*Args` struct from `cli::run`. Do not edit `cli.rs`. If a
  flag is missing or wrong, record it under Contract change requests.
- `crate::commands::{config_error, not_implemented}` are the shared exit
  helpers. Exit codes follow v0.4.1: 0 is ok, 1 is a blocked or failed check,
  and 2 is a usage or configuration error.
- `crate::plan`, `crate::install`, `crate::link`, `crate::validate::compose_validation`,
  and `crate::audit` are implemented and tested.

## Output parity

Text and `--json` output match v0.4.1 for the same situation. Read the v0.4.1
command in `src/my_skills/` and the Python tests in `tests/` that pin its output.
Port each relevant pinned expectation to an `assert_cmd` test. Name each test so
the v0.4.1 test it mirrors is clear. v1-only output (spec §6) is new and gets
its own tests.

## Test harness

Black-box tests run the built binary with:
- `HOME`, `XDG_STATE_HOME`, `XDG_DATA_HOME`, and `XDG_CONFIG_HOME` set to temp dirs
- `MY_SKILLS_ROOT` set to a temp registry

Every target path in a test manifest is inside the temp dir. Write a small
helper module under `tests/` named after your stream, for example
`tests/support_g.rs`. Each stream keeps its own copy, so there are no merge
conflicts.
