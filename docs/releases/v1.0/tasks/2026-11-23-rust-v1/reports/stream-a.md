# Stream A report

## Done

- Implemented `hash_file` and `hash_directory` in `f8a95d7`. File hashing
  streams 64 KiB chunks. Directory hashing walks regular files, applies the
  shared ignore rules, and hashes slash-separated relative paths in UTF-8 byte
  order using the v0.4.1 framing and output format.
- Committed synthetic fixtures, the v0.4.1 Python golden generator, and Rust
  integration tests. The implementation uses the existing dependencies.

## Tests

The tests used temporary HOME and XDG directories.

| Command | Result |
| --- | --- |
| `uv run python crates/my-skills/tests/golden/hash/make_golden.py` | Regenerated all 11 hashes with Python v0.4.1. A repeat run produced identical bytes. |
| `uv run pytest tests/test_hashing.py` | Passed all 5 Python tests. |
| `cargo build` | Passed on macOS. |
| `cargo test` | Passed all 8 tests on macOS, including all 11 golden cases. |
| `rustfmt --edition 2024 $(git ls-files 'crates/*.rs') crates/my-skills/tests/hash_golden.rs` | Passed. A follow-up with `--check` passed. |
| `/opt/homebrew/bin/cargo-clippy clippy --all-targets -- -D warnings` | Passed on macOS. |
| `cargo check --target x86_64-pc-windows-msvc --all-targets --locked` | Passed with unused-dependency warnings from the workspace skeleton. |
| `cargo build --lib --target x86_64-pc-windows-msvc --locked` | Passed. |
| `cargo build --target x86_64-pc-windows-msvc --locked` with `rust-lld` | Reached the executable link step. The linker reported missing Windows SDK import libraries, including `kernel32.lib`. |
| `cargo test --target aarch64-unknown-linux-musl --test hash_golden --no-run --locked` with `rust-lld` | Built the Linux test binary. All 5 tests passed in a local Linux container, including the invalid UTF-8 filename test and all 11 goldens. |
| `git diff --cached --check` | Passed for the implementation and fixtures. |

| Golden case | Result |
| --- | --- |
| `empty` | Matched the empty-directory hash. |
| `single-skill` | Matched with one `SKILL.md`. |
| `nested` | Matched with nested scripts and references. |
| `ignored-entries` | Matched `nested` with `.DS_Store`, `.git/HEAD`, `__pycache__/x.pyc`, and `.omc/state` at root and nested levels. |
| `ds-store-directory` | Matched with a regular file inside a directory named `.DS_Store`. |
| `gitignore` | Matched with `.gitignore` as the only file. |
| `mixed-unicode-order` | Matched with Korean, uppercase, lowercase, digits, punctuation, and nested paths. |
| `binary-over-64-kib` | Matched for a 65,793-byte binary file. |
| `empty-file` | Matched and differed from the empty-directory hash. |
| `empty-subdirectories` | Matched `single-skill` with additional empty subdirectories. |
| `same-tree-different-root` | Matched `nested` under a different root name. |

The direct file hash matched the lowercase, unprefixed SHA-256 for `abc`.
The error tests verified path and error kind for missing paths and invalid
UTF-8. The Unix symlink test covered nested directory and file symlinks and a
symlink used as the walk root.

## Differences from v0.4.1

- **Deliberate.** Rust skips symlinks in directory walks, including a symlink
  used as the root. The Python v0.4.1 probe on Python 3.13 skipped nested
  directory symlinks, included symlinks to regular files via `is_file()`, and
  followed a symlink used as the root. The v0.4.1 import command rejects
  symlinks at the root and inside imported skills.
- **Deliberate.** Rust returns walk I/O failures with the affected path.
  The Python v0.4.1 probe on Python 3.13 returned the empty-directory hash
  for a missing root.
- **Deliberate error type.** Python raises `UnicodeEncodeError` when its
  UTF-8 encoding step encounters a relative filename with surrogate-escaped
  bytes. Rust rejects the same entry with `Error::Io` and includes the path.

## Contract change requests

None.

## Open issues

Windows executable linking and runtime tests remain for Windows CI. The
Windows library build and all-target compile check passed locally.
