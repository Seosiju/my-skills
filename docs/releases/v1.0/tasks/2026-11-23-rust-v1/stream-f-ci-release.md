# Stream F: CI and release pipeline

Read `README.md` in this folder first.

## Goal

Make the Rust crate build and test on Linux, macOS, and Windows in CI, and
prepare a release pipeline and installers for spec §10.3 and §10.4. The Python
CI job keeps working unchanged.

## Files you own

- `.github/workflows/ci.yml` (add the Rust job; keep the Python job as is)
- `.github/workflows/release-rust.yml` (new)
- `scripts/install.sh`, `scripts/install.ps1` (new)
- `crates/my-skills/tests/` files named `ci_*.rs`, if you need any
- `docs/release-checklist.md`: add a Rust section only; leave the Python
  sections unchanged

## CI

1. Triggers: push to `main` and `dev`, pull requests to `main` and `dev`, and
   `workflow_dispatch`.
2. Add a `rust` job with a matrix of `ubuntu-latest`, `macos-latest`, and
   `windows-latest`:
   - `cargo fmt --all --check`
   - `cargo clippy --all-targets -- -D warnings`
   - `cargo test --all-targets`

   Cache cargo. Use the stable toolchain with `rustfmt` and `clippy`
   components.
3. Use the action pinning style the file already uses (`actions/checkout@v7`).
4. The skeleton's `todo!()` stubs are fine, because tests do not call them.

## Release

1. `release-rust.yml` runs on tags matching `v1.*` and on `workflow_dispatch`
   (dry run, no publish).
2. Build these targets: `aarch64-apple-darwin`, `x86_64-apple-darwin`,
   `x86_64-unknown-linux-gnu`, and `x86_64-pc-windows-msvc`.
3. Package each one: `.tar.gz` for Unix and `.zip` for Windows, with the binary,
   `LICENSE`, and `README.md`.
4. Publish a GitHub Release with `SHA256SUMS`. A tag with a pre-release suffix
   (`-alpha.N`, `-beta.N`, `-rc.N`) makes a GitHub pre-release.
5. **Do not create any tag.** Do not run the workflow.

## Installers

1. `install.sh` (POSIX sh, for macOS and Linux):
   - Detect the OS and architecture.
   - Download the latest release, or the one named by `MY_SKILLS_VERSION`.
   - Verify it against `SHA256SUMS`.
   - Install into `${MY_SKILLS_INSTALL_DIR:-$HOME/.local/bin}`.
   - Print a PATH hint when needed.
   - Support `MY_SKILLS_RELEASE_BASE_URL` so tests can point it at a local
     directory.
2. `install.ps1` does the same on Windows into
   `$env:LOCALAPPDATA\Programs\my-skills\bin`. It prints a PATH hint and does
   not change PATH unless `-AddToPath` is passed.
3. Test `install.sh` locally against a fake release directory that you build in
   a temp folder (`file://`). Run `shellcheck` and `actionlint` if they are
   installed, and say in the report whether they were.

## Report this (do not fix it)

v0.4.1 `my-skills update` (`src/my_skills/update_commands.py`) picks the highest
`vMAJOR.MINOR.PATCH` tag and runs `uv tool install git+...@<tag>`. Find out
exactly which tags it would choose once `v1.0.0-alpha.N` and later `v1.0.0`
exist, and what a Python user would then get. Put the answer under Open issues.

## Done when

- The workflow YAML passes `actionlint`, or careful review if it is not
  installed.
- The `install.sh` local test passes.
- The report states what could not be verified without running GitHub Actions.
