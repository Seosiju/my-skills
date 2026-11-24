# Release Checklist

## 1. Preflight

- Check that the diff excludes private config, account IDs, tokens, host
  installations, state files, and personal memory.
- Prepare the release section in `CHANGELOG.md` and check that both READMEs
  describe the same install and first-run flow.
- Select the intended `v1.*` tag and verify that its version without the
  leading `v` matches the package version in `Cargo.toml`. The updater
  expects the installed executable's `--version` to match the release tag.
- Keep the [binary installation instructions](../README.md#install-the-cli)
  on `main` and check the links in the `my-skills` seed skill and the generated
  registry README in `src/registry/seeds.rs`.

## 2. Source Verification

Use a clean checkout of the candidate commit.

```bash
git clone https://github.com/Seosiju/my-skills.git
cd my-skills
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo build --release
```

Confirm CI passes on Ubuntu, macOS, and Windows. To smoke-test CLI commands,
set `MY_SKILLS_ROOT` to a disposable seeded registry with test host paths
and isolate config, state, and data directories from your own.

```bash
cargo run -- doctor --no-update-check
cargo run -- skills --json
cargo run -- bootstrap --dry-run
cargo run -- install my-skills --host codex --dry-run
cargo run -- install my-skills --host agy --dry-run
```

Verify a manifest without `targets.agy` keeps AGY disabled and leaves it out of
default `--host all` plans. New registries explicitly disable AGY while seeded
skills support it. Verify an old manifest that still has `[targets.hermes]`
loads: with a `path` it is a custom target, and a disabled one without a path
is ignored.

On macOS, Antigravity CLI 1.2.16 discovered regular folders and symlinks at
`~/.gemini/config/skills` with the non-interactive command
`agy -p '/skills' --output-format json`. The check used zero model turns and
tokens. Verify discovery again for other Antigravity CLI versions.

## 3. Package and Test Before Tagging

GitHub requires `.github/workflows/release-rust.yml` on the default branch
(`main`) before **Release Rust** can be dispatched manually.
[GitHub's manual workflow requirements](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/manually-run-a-workflow#configuring-a-workflow-to-run-manually)

Use the following pre-tag artifact checks once `main` CI passes.

1. Run **Release Rust** with `workflow_dispatch` on the candidate commit. The
   run builds and uploads the `rust-release` artifact.
2. Download and unpack that artifact. Verify `SHA256SUMS` with
   `sha256sum --check SHA256SUMS` on Linux or `shasum -a 256 -c SHA256SUMS`
   on macOS. Check all four targets (macOS Apple silicon and Intel, Linux
   x86-64 GNU, and Windows x86-64 MSVC). Each archive should contain
   `my-skills` (or `my-skills.exe`), `LICENSE`, and `README.md`.
3. In each target environment, set `MY_SKILLS_RELEASE_BASE_URL` to the local
   artifact directory as a `file://` URL and `MY_SKILLS_INSTALL_DIR` to a
   disposable directory. The URL override takes precedence over tag
   selection. From the candidate checkout, run `sh scripts/install.sh` on
   macOS or Linux or `powershell -File scripts/install.ps1` on Windows.
   Run the installed executable by its full path with `--version` and check
   that it matches the intended tag. Check the same isolated registry flow
   used for the source build.

## 4. Publish and Verify the Release

1. Create and push the approved `v1.*` tag. The tag-triggered **Release Rust**
   run publishes the GitHub Release with the four archives and `SHA256SUMS`.
   Tags with a hyphen publish as prereleases. Stable tags are marked Latest.
2. Repeat the installer smoke test using the published assets. Clear
   `MY_SKILLS_RELEASE_BASE_URL` and set `MY_SKILLS_VERSION` to the exact
   published tag. With neither override, the installers use the latest
   GitHub release URL. Check that default after a stable Rust release is
   the latest release.
3. From an older Rust executable in a disposable install directory, run
   `my-skills update --check` and `my-skills update --dry-run`. Use
   `--channel main` for a prerelease candidate. Once the preview selects
   the intended version, run `update` with the same channel and confirm
   the replacement's `--version`. The stable channel selects only stable
   v1 releases.

## 5. Recovery

- Keep published tags intact. Verify the fix before shipping a new release.
- Record the failing command, expected behavior, and actual behavior in a
  follow-up issue.
- For secret exposure or unsafe writes, follow [SECURITY.md](../SECURITY.md)
  and use a GitHub security advisory.
