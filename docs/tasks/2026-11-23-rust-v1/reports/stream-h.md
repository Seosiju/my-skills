# Stream H report

## Done

- Ported `init-registry`, `skills`, `share`, `import`, `enable`, and
  `disable` in `83f7c54`. Catalog rows use `plan::status_of` and retain
  filtering, per-host statuses, invalid metadata, and governance fields.
- Ported the line-based manifest editor. It preserves surrounding comments and
  formatting, checks the resulting TOML, and replaces files atomically while
  preserving permissions. The golden tests include v0.4.1’s ordering when a
  section is missing both keys.
- Import and share stage the source with no-follow directory and file opens,
  validate and audit the staged content, and publish through rename. Share
  records the source host as an adopted copy. A failed state save restores the
  previous manifest and canonical directory.
- Embedded the six files allowlisted by v0.4.1. The scaffold preserves README
  handling, active-root caching, seed settings, and Git behavior. The embedded
  payload retains `source_revision = "0.4.1"`. The cap crates provide
  cross-platform no-follow opens and offer MIT licensing, as do `include_dir`
  and `tempfile`.
- The following sentences in the public seed reflect the old copy model.
  The embedded text is byte-for-byte unchanged.

  > Host directories (~/.claude/skills, ~/.agents/skills, ~/.hermes/skills,
  > ~/.gemini/config/skills) are **build outputs**: `install` and `sync`
  > copy the canonical skill into them.

  > After import, edit the canonical registry copy and use `sync` to update
  > managed host installs.

  The neighboring drift sentence still applies to copy connections. The two
  import examples also use `install` where the v1 CLI hint uses `sync`.
  [Seed SKILL.md](../../../../skills/my-skills/SKILL.md)

## Tests

All registry CLI and Python comparison runs used synthetic temp registries,
HOME, and XDG paths. Git tests used synthetic identities and isolated config.

| Command | Result |
| --- | --- |
| `uv run --locked python` with the v0.4.1 manifest editor | Generated five byte goldens for CRLF, comments and spacing, missing tables or keys, and a same-value update. Rust matched all five. |
| `uv run --locked python` with the v0.4.1 and Rust CLI comparison | Matched stdout, stderr, and exit status for scaffold creation, catalog JSON, share plan and apply, toggles, and force import after accounting for the sync hints. The README, ignore file, and manifest matched after accounting for the link default. |
| `uv run --locked python -m pytest -q` for the eight Python suites in the brief | 61 passed. |
| `cargo build` | Passed on macOS. |
| `cargo test --quiet` | 133 passed, including 64 Stream H tests. Five install/sync CLI integration tests are ignored until the integrator wires those handlers. |
| `rustfmt --edition 2024 --check` on tracked and new Rust files | Passed. |
| `/opt/homebrew/bin/cargo-clippy clippy --all-targets -- -D warnings` | Passed on macOS. |
| `cargo check --target x86_64-pc-windows-msvc --all-targets` | Passed using a worktree-local Rust toolchain. |
| `cargo build --lib --target x86_64-pc-windows-msvc` | Passed. |
| `git diff --cached --check` | Passed. |

## Differences from v0.4.1

- **Deliberate.** New manifests use `install_mode = "link"`, per D1.
- **Deliberate.** Import hints use `sync`. Share apply adds the matching
  hint after the adoption line. The idempotent import path retains its two
  success lines.
- **Deliberate.** Share blocks symlinks and Windows junctions at or inside a
  source. Plans identify these as error risks with a `skip` recommendation and
  an empty content hash. Import keeps its symlink diagnostic and can emit it
  before reading the root skill metadata. Import also reports an explicit
  block when the destination is a file or link and `--force` is supplied.
  Share can replace such a canonical entry with `--force` while preserving a
  link’s target.
- **Deliberate.** Share honors the manifest audit threshold in the plan and
  apply, including thresholds that make a validation warning block. The
  standard validation pass still runs with `--skip-audit`, as in v0.4.1.
- **Deliberate.** Source paths that overlap machine-local skill data, invalid
  share path components, unsupported filesystem entries, and writes that
  overlap host directories get explicit errors. Manifest edits can report a
  busy writer or a resulting invalid TOML configuration. Filesystem failures
  use the shared Rust diagnostics.
- **Deliberate.** Catalog uses v1 planner semantics, including `BROKEN` for
  a managed link whose canonical target has disappeared.
- **Shared CLI difference.** Clap supplies parse-time usage text. The
  conflicting-flag cases still return exit code 2.

## Contract change requests

- Spec §6 calls for `--dry-run` on writes. The parsed argument structs offer
  `share --plan` for preview and omit `--dry-run` for `share`, `import`,
  `init-registry`, `enable`, and `disable`. The integrator owns the flags
  and any preview output contract.

## Open issues

- The integrator owns the shared `Cargo.lock` update and the five ignored
  install/sync tests. Stream H was verified with a locally resolved lockfile.
- Windows executable linking and runtime coverage remain with Windows CI.
- An interrupted manifest writer can leave `my-skills.toml.edit-lock`.
  Registry writers report the lock path if it already exists. State writers
  outside Stream H use the shared state module and its separate staging file.
