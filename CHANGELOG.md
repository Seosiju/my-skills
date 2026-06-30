# Changelog

All notable changes to this project will be documented in this file.

This project follows a GitHub-first release flow. Git tags and GitHub Releases
are the release source of truth while PyPI publishing remains undecided.

## [Unreleased]

### Added

- GitHub Actions CI for tests, package build, and CLI release smoke checks.
- Contributor, security, changelog, and release checklist documentation.
- GitHub-first install documentation using `uv tool install
  git+https://github.com/Seosiju/my-skills.git`.

## [0.1.0] - 2026-06-30

### Added

- Initial cross-agent Agent Skill registry CLI.
- Canonical `skills/` source directory with install, sync, status, share,
  import, bootstrap, doctor, validation, and data-path commands.
- Host support for Claude Code, Codex, and Hermes.
- Copy and link install modes with drift, conflict, unmanaged, stale, missing,
  and fresh status reporting.
- Machine-local data guidance for private config and memory.
