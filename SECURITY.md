# Security Policy

## Supported Versions

Security fixes target the current `main` branch and the latest GitHub release.
This project is pre-1.0, so older releases may not receive separate patch
branches.

## Reporting a Vulnerability

Please report suspected vulnerabilities privately by opening a GitHub security
advisory for this repository:

https://github.com/Seosiju/my-skills/security/advisories/new

If advisories are unavailable, email the repository owner listed on the GitHub
profile and avoid posting exploit details in a public issue.

Useful details include:

- The affected command, skill, or file path.
- Exact reproduction steps.
- Whether the issue can expose secrets, overwrite files, install untrusted
  content, or execute unintended commands.
- The operating system, install method, and output of `my-skills --version`.

## Scope

In scope:

- Secret, token, account, or machine-local path exposure in canonical skills.
- Unsafe writes to host skill directories.
- Drift, conflict, or unmanaged destination handling that can overwrite user
  edits.
- Import/share/install/sync behavior that can install untrusted content without a
  clear plan or dry-run path.

Out of scope:

- Vulnerabilities in a user's private local skills that are not part of this
  repository.
- Host tools such as Claude Code, Codex, Hermes, or Antigravity CLI outside
  the files written by `my-skills`.
- Public information already intentionally committed to the repository.

## Release Hygiene

Before a Rust release, maintainers should run the checks in
[docs/release-checklist.md](docs/release-checklist.md).

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo build --release
cargo run -- doctor --no-update-check
cargo run -- skills --json
cargo run -- bootstrap --dry-run
cargo run -- install my-skills --host hermes --dry-run
```

Use a disposable registry and test host paths for CLI smoke checks.
