# Changelog

All notable changes to this project will be documented in this file.

This project follows a GitHub-first release flow. Git tags and GitHub Releases
are the release source of truth while PyPI publishing remains undecided.

## [Unreleased]

### Added

- `sync --dry-run --json` for machine-readable sync previews (`sync --json`
  without `--dry-run` exits `2`; `sync --check` takes precedence over
  `--dry-run` and cannot be combined with `--json`).
- Structured fields in `install --dry-run --json` and `sync --dry-run --json`:
  `registry` (`{path, source}`), `dry_run`, `blocked`, `applicable` (whether
  the same command without `--dry-run` would exit `0`, including `--yes` and
  active-registry checks), `block_reasons` (`validation`, `audit`, `needs_yes`,
  `inactive_registry`, or a lowercased block action such as `block_drift` or
  `block_foreign`), and `validation` (`[{skill, host, ok, errors, warnings}]`),
  while keeping the existing `actions` and `audit` keys.
- `registry` (`{path, source}`) metadata in `skills --json` and
  `share --plan|--dry-run --json`.
- Cross-registry connection ownership protection (`FOREIGN` status and
  `BLOCK_FOREIGN` action): connections whose `state.json` record points into
  another registry's `skills/` directory are never pruned, updated, converted,
  uninstalled, or overwritten. `sync` silently leaves another registry's
  connections alone when pruning, and `share --plan` recommends `skip` for
  foreign-owned host copies.
- Complete option descriptions across `--help`, plus top-level registry
  resolution, default host scope, and exit-code rules.

### Changed

- **Deliberate behaviour change — write refusal from a non-active cwd
  registry:** when the current directory's `my-skills.toml` differs from the
  saved active registry, real write commands (`install`, `sync`, `enable`,
  `disable`, `import`, `share`, `uninstall`, `convert`, `bootstrap`, and
  `hooks install`/`uninstall`) now refuse with exit code `2` instead of writing
  to the cwd registry. The error names both registries and the three ways out:
  run from the active registry, run `my-skills set-root <path>`, or set
  `MY_SKILLS_ROOT=<path>` for one command. Read-only commands still run against
  the cwd registry with a stderr note.
- **Deliberate behaviour change — `sync --dry-run` validation and audit
  parity:** `sync --dry-run` now runs the same validation and audit checks as a
  real `sync`, and exits `1` when validation fails (matching
  `install --dry-run`; otherwise `0` even when audit, plan items, `--yes`, or
  an inactive cwd registry would refuse the real run). Use `sync --check` when
  you need a non-zero exit code for any out-of-date state.
- Read-only and preview commands (`validate`, `audit`, `status`, `skills`,
  `data-path`, `share --plan`/`--dry-run`, `sync --check`, every `--dry-run`,
  and usage errors) never write the active-registry cache. Only the first real
  write from a registry — including `bootstrap` (unless `--dry-run` or
  `--skip-skill-install`) and `hooks install`/`uninstall` — records it.
- Text dry-runs (`install`/`sync --dry-run`) print a top-line `BLOCKED:`
  summary whenever items or an inactive cwd registry would refuse the real run,
  and end with `Re-run with --yes to apply: ...` when multi-host confirmation
  is required. With `--json`, validation warnings stay in JSON instead of being
  printed to stderr.
- JSON output (`skills --json`, `share --plan|--dry-run --json`, and dry-run
  JSON) now emits plain UTF-8 instead of `\uXXXX` escapes (roughly 45% smaller
  for non-ASCII descriptions). Audit `result_hash` fingerprints are unchanged.

### Fixed

- `sync` no longer prunes or overwrites copy or link connections installed by
  another registry on the same machine, and `sync --check` no longer treats
  `FOREIGN` records on hosts a skill does not target as drift.
- `audit.skipped` in dry-run JSON is now `true` only when `--skip-audit` is
  passed (or `[audit] enabled = false`), and `sync --dry-run --skip-audit`
  prints the skipped-audit warning in text output.
- Windows paths in JSON and CLI output (`doctor`, `skills`, `share`, `audit`,
  `install`/`sync --dry-run`, `hooks`, stderr notes, manifest errors,
  `data-path`, and `update --dry-run`) strip the `\\?\` verbatim prefix and use
  consistent `\` separators.

## [1.0.3] - 2026-11-24

### Removed

- Hermes is no longer a supported host. The built-in hosts are Claude Code,
  Codex, and Antigravity CLI. Old registries keep loading: a `[targets.hermes]`
  table with a `path` becomes an ordinary custom target, a disabled one without
  a path is ignored, and existing Hermes copies are left in place.

## [1.0.2] - 2026-11-24

### Fixed

- Skills can bundle common font and image assets whose file signatures match
  their extensions without triggering NUL-byte errors during import, validation,
  audit, or pre-commit checks.

## [1.0.1] - 2026-11-24

### Fixed

- If saving `state.json` fails while `install`, `sync`, `bootstrap`, or
  `uninstall` changes a host entry, that change is now rolled back. Rerunning
  the same command after fixing the cause succeeds instead of stopping with
  `BLOCK_CONFLICT`.
- `--dry-run` no longer writes the active-registry cache on a first run. The
  preview prints `would remember <path> as the active registry` instead, and
  still notes when another registry is active.
- `doctor` tells you to find or fix the registry first when it cannot read the
  manifest, and `bootstrap --dry-run` describes the hook step the way the real
  run does.
- Windows output: hook install paths no longer mix `/` and `\`, `set-root` no
  longer shows the `\\?\` prefix, and audit reports use `/` in file paths.

## [1.0.0] - 2026-11-24

- First stable release of the Rust CLI. It manages a canonical skill registry
  and links or copies skills into Claude Code, Codex, Hermes, and Antigravity CLI.
- Install prebuilt binaries for macOS (Apple silicon and Intel), Linux
  (x86-64 GNU), and Windows (x86-64 MSVC) with the release installers.
- Use `my-skills update --check` to check for a newer stable v1 release.
  Python v0.4.2 users can run `my-skills update` for migration instructions.

## [1.0.0-alpha.1] - 2026-11-24

### Added

- First Rust prerelease, with binaries and SHA-256 checksums for macOS
  (Apple silicon and Intel), Linux (x86-64 GNU), and Windows (x86-64 MSVC).
- Symlink and junction installation for skills, with copy fallback and
  per-host install modes.
- Shell and PowerShell installers and a Rust self-updater. Use
  `my-skills update --check --channel main` to check for prereleases.

## [0.4.2] - 2026-11-23

### Changed

- `my-skills update` no longer installs a Rust v1 tag through uv. On the
  stable channel, when a `v1.*` release exists, it stays on the newest
  compatible 0.4.x release and prints the Rust binary install steps instead.
  On the main channel, once any `v1.*` tag exists (prereleases included), it
  stops installing from `main` and prints the same steps. Existing installs
  are left untouched.
- The install links in that notice point at the release tag, so they resolve
  before and after the Rust code reaches `main`.
- `my-skills doctor` notes on its `Update:` line when a Rust v1 release is
  available.

## [0.4.1] - 2026-11-14

### Removed

- `personal-profile` from default registry seeds, the source-checkout skill
  manifest, and wheel/source distributions. New registries and source-checkout
  bootstrap installs no longer include this skill. Updating the CLI package
  preserves skills in separate personal registries, host installations, and
  machine-local data.

## [0.4.0] - 2026-10-03

### Added

- Antigravity CLI (`agy`) host support for discovery diagnostics, validation,
  install/sync, status/catalog, sharing, and uninstall through the existing
  skill management workflow.
- AGY user skills path `~/.gemini/config/skills` and workspace path
  `.agents/skills`, with copy and symlink discovery verified on CLI 1.2.16.
- Explicitly disabled AGY targets in newly initialized registries, and AGY in
  the seed skills' supported-host lists for opt-in activation.
- Regression coverage and source/installed-tool CI smoke checks for AGY.

### Changed

- Default target paths and activation now use the shared host contract. AGY
  stays disabled when omitted from a manifest; existing Claude Code, Codex,
  and Hermes activation defaults are preserved.
- Public setup and release documentation now includes the AGY opt-in flow.

## [0.3.1] - 2026-07-09

### Added

- `doctor` now reports whether the installed CLI is behind the latest stable
  release tag, with `doctor --no-update-check` for offline diagnostics.
- `update` command for explicitly updating the installed CLI to the latest
  stable release tag, plus an opt-in `--channel main` development channel.

## [0.2.0] - 2026-07-09

### Added

- `set-root` command for explicitly selecting the active registry root.
- `import --enable` for registering an imported skill as enabled immediately.
- `--version` flag, and expanded `doctor` output with the active registry root
  source, CLI version, skill counts, and state/data paths.
- GitHub Actions CI for tests, package build, and CLI release smoke checks.
- Contributor, security, changelog, and release checklist documentation.
- GitHub-first install documentation using `uv tool install
  git+https://github.com/Seosiju/my-skills.git`.

### Changed

- Running a command inside a `my-skills` directory no longer silently repoints
  the machine-wide active registry when another valid active root is cached; use
  `my-skills set-root` or `init-registry` to switch. First-run discovery still
  records the root when none is cached.
- `import` now registers copied skills in `my-skills.toml` automatically. New
  imports default to disabled unless `--enable` is passed, and re-importing an
  existing registered skill preserves its hosts and local-overlay separation.
- `import` now rejects source directories that contain symlinks, preventing the
  copy step from pulling files outside the audited source tree.
- **Breaking:** `validate` — and the always-on validation that runs before
  `install`, `sync`, `import`, and `share` — now applies the full audit
  analyzer set with a fixed internal policy. Skills that previously passed
  may now be reported:
  - Absolute user/home paths in any file are now **errors** (previously
    warnings, and previously only checked in `SKILL.md`).
  - Audit-only rules (prompt injection, dangerous command patterns,
    credential/network dataflow) now surface as `validate` **errors**.
  - MEDIUM-severity rules (e.g. network senders) now surface as warnings.
- `share --plan` no longer lists the same finding twice; validation- and
  audit-derived entries are de-duplicated by file and message.
- Runtime and system artifacts (`__pycache__`, `.git`, `.omc`, `.DS_Store`)
  are now ignored consistently by content hashing, validation scanning,
  audit, and install copies. A stray `.pyc` file in a skill directory no
  longer breaks `validate`, `audit --all`, or `install`.

### Fixed

- Running the test suite no longer overwrites the real user's my-skills state
  file and active-root cache.
- `init-registry` now creates the scaffold commit even when the target is
  already an empty git repository.
- `install`/`sync` no longer lose install state when a plan item fails
  mid-run: each successful item is recorded immediately and failures are
  reported per item with a non-zero exit.
- `skills`/`status` no longer abort when one skill has broken metadata; the
  broken row is marked `invalid` and the command still exits 0 (the JSON
  output keeps its contract for agent consumers).
- A state file written by a newer my-skills version, or with malformed
  records, is now rejected with a clear message instead of a raw traceback.
- `share` now honors the manifest audit policy when evaluating candidates
  (previously it always used the default policy).
- Package description no longer claims Gemini CLI support; supported hosts
  are Claude Code, Codex, and Hermes.

### Removed

- Internal `security.py` module; its rules now live in the audit analyzers
  (user-visible effects are listed under "Changed").

## [0.1.0] - 2026-06-30

### Added

- Initial cross-agent Agent Skill registry CLI.
- Canonical `skills/` source directory with install, sync, status, share,
  import, bootstrap, doctor, validation, and data-path commands.
- Host support for Claude Code, Codex, and Hermes.
- Copy and link install modes with drift, conflict, unmanaged, stale, missing,
  and fresh status reporting.
- Machine-local data guidance for private config and memory.
