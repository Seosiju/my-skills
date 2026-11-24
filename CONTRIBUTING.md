# Contributing

Use the [release installers](README.md#install-the-cli) to install the Rust
`my-skills` CLI. Use the source setup below when contributing changes.

## Development Setup

Use Git and stable Rust with the `rustfmt` and `clippy` components. On
Windows, use the MSVC toolchain and build tools.

Clone the repository and run the test suite:

```bash
git clone https://github.com/Seosiju/my-skills.git
cd my-skills
cargo test --all-targets
```

Use `cargo run -- <cmd>` from the working tree. For checks that resolve a
registry, point `MY_SKILLS_ROOT` to a disposable registry with test host paths.

```bash
cargo run -- doctor --no-update-check
cargo run -- skills --json
cargo run -- bootstrap --dry-run
cargo run -- install my-skills --host hermes --dry-run
```

## Contribution Guidelines

- Keep canonical skills under `skills/<name>/`.
- Keep host install directories, state, accounts, tokens, and local memory out of
  git.
- Put machine-local overrides in `my-skills.local.toml`, `local/`, or the data
  directory returned by `my-skills data-path <skill>`.
- Prefer dry-run commands before commands that write to host directories.
- Keep changes small and covered by tests when behavior changes.
- Do not add runtime dependencies without opening an issue or PR discussion first.

## Before Opening a Pull Request

Run:

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo build --release
```

For release-related changes, also check `docs/release-checklist.md`.
