# Stream I — Self-update and Python migration guard

## Done

- Rust resolves published GitHub Releases by semantic version. Stable selects
  the highest non-prerelease `v1.*` tag. Main also considers prereleases.
  The CLI reports its current version, channel, and selected release.
  [GitHub Releases API](https://docs.github.com/en/rest/releases/releases#list-releases)
- The updater selects the release workflow's archive for the running target,
  verifies it against `SHA256SUMS`, and stages the executable beside the
  install. It serializes replacements with an advisory lock, keeps a hard
  link backup on Unix, and renames the old executable to `.old` on Windows.
  It launches the replacement with `--version` and restores the previous
  executable if the swap or verification fails.
- HTTP uses `ureq` with rustls on Unix and Schannel with system trust on
  Windows. The doctor lookup has a two-second HTTP budget.
  `MY_SKILLS_RELEASE_API_BASE_URL` overrides the API origin
  (`/repos/Seosiju/my-skills/releases` is appended).
  `MY_SKILLS_RELEASE_BASE_URL` points directly to the asset directory,
  following the installers. Both support `file://` fixtures.
  [ureq documentation](https://docs.rs/ureq/3.5.2/ureq/)
- The Python bridge selects the newest compatible stable 0.4.x tag from
  the same Git lookup that found the migration release. When a stable tag
  is v1 or later, regular update and dry-run print the migration links and
  exit 0 before looking up uv.
- Implementation commits are `917b2c2` and `b4c61c6`.

## Tests (commands and results)

- `cargo build` passed.
- `cargo test` passed all 78 tests. Nine stream I tests cover channel
  selection and check exit codes, dry-run output and filesystem effects,
  successful replacement, checksum mismatch, failed download, version
  verification rollback, and the doctor skip path.
- Direct `rustfmt --edition 2024` and `--check` passed on tracked Rust files
  and both new test files.
- `/opt/homebrew/bin/cargo-clippy clippy --all-targets -- -D warnings` passed.
- `cargo check --locked --all-targets --target x86_64-pc-windows-msvc`
  passed using a worktree-local Rust 1.100.0 toolchain copy with the Windows
  standard library. This checked the Windows ZIP, replacement, and test code.
- `uv run --locked --python 3.11 pytest -q` passed all 308 tests with a
  temporary HOME, XDG directories, and pytest base directory. The new cases
  cover stable v1 and v2 migration, compatible 0.4.x check results, and a
  prerelease-only cutover. The update tests used local fixtures and stubs.
- Ran the original Python updater from the baseline with synthetic tags
  and stubbed Git, uv, and `--version` to confirm the legacy output below.

## Differences from v0.4.1

- Deliberate. Rust stable uses published v1 releases. Python stable resolves
  Git tags and installs source with uv. The Rust preview lists download
  URLs and the executable path. Rust adds a `Channel:` line while keeping
  check exits 1 for available, 0 for current or ahead, and 2 for failed
  lookup. Doctor gives failed lookups an `unknown (...)` status.
- Deliberate. Python `main` refers to the head of the Git branch and compares
  commit IDs. Rust `main` refers to the highest published semantic version
  including prereleases and compares versions. A branch commit becomes an
  update candidate once it has a published release.
- Deliberate. The guarded Python stable updater reports the migration link
  while keeping the check exit code tied to the compatible Python release.

With stable tags `v0.4.2` and `v1.0.0` available, the stock v0.4.1 user's
`update --check` exits 1 and prints:

```text
Current: my-skills 0.4.1
Latest:  v1.0.0
Update available
```

Their `update --dry-run` exits 0 and displays:

```text
Current: my-skills 0.4.1
Latest:  v1.0.0
Would run: uv tool install --force git+https://github.com/Seosiju/my-skills.git@v1.0.0
```

Their plain `update` prints the same current and latest lines and runs the
displayed uv command after printing `Updating via …`. If the tagged tree
still packages Python 0.4.1, uv reinstalls that package and the updater
exits 1 with `error: updated command did not report expected version 1.0.0:
my-skills 0.4.1`. If it packages a different Python version, the mismatch
reports that version. If the tag lacks Python packaging, the uv command
fails and the updater exits 1.

When released as v0.4.2, the guarded user's `update --check` exits 0 and
prints:

```text
Current: my-skills 0.4.2
Latest:  v0.4.2
Already up to date
my-skills v1.0.0 is available.
```

Their plain `update` and `update --dry-run` each exit 0 and print:

```text
Current: my-skills 0.4.2
my-skills v1.0.0 is available. Install the Rust binary with:
  macOS/Linux: https://github.com/Seosiju/my-skills/blob/main/scripts/install.sh
  Windows:     https://github.com/Seosiju/my-skills/blob/main/scripts/install.ps1
  Migration spec: https://github.com/Seosiju/my-skills/blob/main/docs/2026-11-23-skill-drive-rust-v1-spec.md
```

## Contract change requests

None.

## Open issues

- Native Windows execution and hosted GitHub Release downloads remain to
  be verified. Windows can keep the running image locked through process
  exit. In that case the renamed `.old` file remains in the staging directory.
- The integrator owns the Python 0.4.2 version bump and release.
