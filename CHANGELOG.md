# Changelog

All notable changes to this project will be documented in this file.

This project follows a GitHub-first release flow. Git tags and GitHub Releases
are the release source of truth while PyPI publishing remains undecided.

## [Unreleased]

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
