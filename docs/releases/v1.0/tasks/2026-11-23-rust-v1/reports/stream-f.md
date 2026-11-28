# Stream F — CI and release

## Done

- Added stable Rust formatting, Clippy, and tests to CI on Ubuntu, macOS, and
  Windows with Cargo caching. CI covers pushes and pull requests for `main`
  and `dev`, plus manual dispatch. Verified the Python job byte for byte
  against the baseline.
- Added a release workflow that builds macOS arm64 and x86_64, Linux x86_64,
  and Windows x86_64. Each archive contains the binary, license, and README.
  The workflow assembles the four archives and `SHA256SUMS` for both triggers.
  Only tag pushes publish a GitHub Release. The publish job marks `-alpha.N`,
  `-beta.N`, and `-rc.N` tags as prereleases. Both macOS targets use the same
  runner image, with the Intel target cross-compiled on Apple Silicon.
- Added POSIX sh and PowerShell installers with release selection, SHA-256
  verification, and staged binary replacement. The base URL override points
  directly to an asset directory. Windows PATH updates require `-AddToPath`.
  Added the Rust release procedure to the existing checklist.
- Implementation commits on `dev-stream-f` are `5408add` and `5213f3d`.

## Tests (commands and results)

- `cargo build`, `cargo test`, direct `rustfmt --edition 2024` on tracked Rust
  sources, and `/opt/homebrew/bin/cargo-clippy clippy --all-targets -- -D
  warnings` passed. The Rust suite ran four tests.
- On rustup stable 1.100.0, `cargo fmt --all --check`, `cargo clippy
  --all-targets -- -D warnings`, and `cargo test --all-targets` passed locally.
  Cargo emitted manifest warnings about unused skeleton dependencies.
- `cargo check --locked --all-targets --target` passed for
  `x86_64-pc-windows-msvc` and `x86_64-unknown-linux-gnu`.
  `cargo +stable build --locked --release --target <target> --bin my-skills`
  built both macOS architectures. Both binaries reported `1.0.0-alpha.0`,
  with the Intel binary running under Rosetta. The local builds warned that
  `rust-objcopy` could not load `libLLVM.dylib` to strip debug info.
- `sh -n scripts/install.sh` and local `file://` installs passed. Verified the
  default destination, a custom path with spaces, replacement, PATH hints,
  and rejection of a tampered archive and missing or duplicate checksums.
  Failed verification preserved the installed binary. A local install from
  the built arm64 release archive reported `1.0.0-alpha.0`.
- `pwsh -NoProfile -NonInteractive -File <fixture-test.ps1>` passed on macOS
  with synthetic Windows environment variables and a local ZIP. Verified
  default and custom destinations, replacement, rejection of a tampered ZIP,
  and PATH hint and session update behavior. Windows user PATH persistence
  requires native Windows verification.
- Ran the workflow packaging and checksum commands locally with synthetic
  binaries. All four archives had the expected entries and `sha256sum --check
  SHA256SUMS` passed. A stub `gh` verified publish arguments for alpha, beta,
  RC, and stable tags.
- `shellcheck scripts/install.sh` and `actionlint` on both workflows passed.
  Neither linter was installed on `PATH`. Verified temporary upstream copies
  of ShellCheck 0.11.0 and actionlint 1.7.12 were used.
- `uv run --locked --python 3.11 pytest -q --basetemp <temporary-dir>` passed
  all 301 Python tests. The fixture directory was outside the tool repo so
  registry ancestor discovery used only the synthetic fixtures. `uv build`
  built the Python 0.4.1 wheel and sdist. An isolated `uv tool install --force`
  from the local Git revision produced `my-skills 0.4.1`.

## Differences from v0.4.1

- Deliberate. Rust v1 adds checksum-verified binary archives and installers
  through GitHub Releases. The archive-based install requires no Python or uv.
- Deliberate. During the alpha period the installers require an explicit Rust
  tag while the repository's latest stable release is Python 0.4.x. The
  default lookup uses GitHub's latest stable release endpoint.

## Contract change requests

None.

## Open issues

- **Python updater at the stable Rust cutover.** Observed remote stable tags
  were `v0.2.0`, `v0.3.1`, `v0.4.0`, and `v0.4.1`. The Python updater matches
  only `refs/tags/vMAJOR.MINOR.PATCH` and compares the three numeric fields.
  With any `v1.0.0-alpha.N` tags added, it selects `v0.4.1`. An installed
  0.4.1 reports that it is up to date. With `v1.0.0` added, it selects
  `v1.0.0` and runs `uv tool install --force
  git+https://github.com/Seosiju/my-skills.git@v1.0.0`. If the tagged tree
  retains the frozen Python package, uv reinstalls Python 0.4.1 from that
  tree and skips the GitHub Release assets. The post-install check expects
  `my-skills 1.0.0`, sees `my-skills 0.4.1`, and exits 1 after reinstalling.
  If the tagged tree removes the Python package, the uv install fails.
  Verified selection with synthetic tag refs and the version mismatch with
  the isolated Git install above.
- **Hosted verification.** The GitHub Actions runner matrix, native Linux
  and Windows linking and execution, Cargo cache restoration, artifact
  transfer, GitHub Release creation, and public release downloads remain
  pending. Work was committed locally on the stream branch without creating
  a tag, pushing, or dispatching a workflow.
