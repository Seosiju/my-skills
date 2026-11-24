# Changelog

All notable changes to this project will be documented in this file.

This project follows a GitHub-first release flow. Git tags and GitHub Releases
are the release source of truth while PyPI publishing remains undecided.

## [Unreleased]

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
