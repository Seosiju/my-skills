# Release Checklist

Use this checklist for GitHub-first releases of `my-skills`.

## 1. Preflight

- Confirm the release is intended for GitHub Releases. PyPI publishing is not
  part of Phase 1.
- Confirm no private config, account IDs, tokens, host install copies, state
  files, or personal memory are included in the diff.
- Confirm `CHANGELOG.md` has an entry for the release.
- Confirm `README.md` and `README.ko.md` still document the current install
  path and first-run flow.

## 2. Clean Clone Install Path

The public install path starts from a clean canonical clone:

```bash
git clone https://github.com/Seosiju/my-skills.git
cd my-skills
uv tool install git+https://github.com/Seosiju/my-skills.git
```

After installing, the user-facing release smoke path is:

```bash
my-skills bootstrap --dry-run
my-skills doctor
my-skills skills --json
my-skills install my-skills --host hermes --dry-run
my-skills install my-skills --host agy --dry-run
```

For local release verification from a working tree, use:

```bash
uv run my-skills bootstrap --dry-run
uv run my-skills doctor
uv run my-skills skills --json
uv run my-skills install my-skills --host hermes --dry-run
uv run my-skills install my-skills --host agy --dry-run
```

## 3. Required Verification

Run:

```bash
uv run pytest
uv build
uv run my-skills doctor
uv run my-skills update --check
uv run my-skills update --dry-run
uv run my-skills skills --json
uv run my-skills bootstrap --dry-run
uv run my-skills install my-skills --host hermes --dry-run
uv run my-skills install my-skills --host agy --dry-run
```

Then verify the GitHub install path from the same clean clone:

```bash
uv tool install --force git+https://github.com/Seosiju/my-skills.git
my-skills bootstrap --dry-run
my-skills doctor
my-skills update --check
my-skills update --dry-run
my-skills skills --json
my-skills install my-skills --host hermes --dry-run
my-skills install my-skills --host agy --dry-run
```

The GitHub Actions workflow must pass the same test, build, source smoke, and
tool-install smoke checks.

For AGY support, also verify an isolated old manifest without `targets.agy`
keeps AGY disabled and absent from default `--host all` plans. New-registry
scaffolds must explicitly disable AGY while seeded skills support it. Do not
repoint a maintainer's private registry for public smoke tests.

Native discovery of regular folders and symlinks at `~/.gemini/config/skills`
was verified with Antigravity CLI 1.2.16 using `agy -p '/skills'
--output-format json` (zero model turns/tokens). Recheck discovery for different
versions; do not claim Antigravity IDE validation. Preserve unmanaged installs
before a migration, and retain existing Hermes regression checks.

## 4. Tag and Release

1. Update `CHANGELOG.md` by moving relevant items from `[Unreleased]` into the
   new version section.
2. Create a signed or annotated tag when possible:

   ```bash
   git tag -a v0.1.0 -m "Release v0.1.0"
   git push origin v0.1.0
   ```

3. Create a GitHub Release from the tag.
4. Paste the changelog section into the GitHub Release notes.
5. Attach build artifacts only if the workflow produced artifacts for that
   release.

## 5. Recovery

If a release is bad:

- Do not force-push or retag a published version.
- Open a follow-up issue with the failing command, expected behavior, and actual
  behavior.
- Ship a new patch tag after the fix is verified.
- If the problem can expose secrets or overwrite user data, follow
  `SECURITY.md` and use a GitHub security advisory.

## 6. Rust v1 Release

1. Confirm that CI passes on `main` or `dev` for Ubuntu, macOS, and Windows.
2. Run **Release Rust** with `workflow_dispatch` before tagging. This builds and
   packages all four targets and uploads the `rust-release` workflow artifact.
3. Download the workflow artifact. Run `sha256sum --check SHA256SUMS` on Linux
   or `shasum -a 256 -c SHA256SUMS` on macOS. Confirm each archive contains
   `my-skills` (or `my-skills.exe`), `LICENSE`, and `README.md`. Confirm
   `my-skills --version` matches the planned tag on each matching platform and
   smoke-test the installers with temporary install directories.
4. Push the approved `v1.*` tag when ready to release. The tag-triggered run
   attaches all four archives and `SHA256SUMS` to the GitHub Release. Tags ending
   in `-alpha.N`, `-beta.N`, or `-rc.N` publish as prereleases.

From a tool repo checkout, run `sh scripts/install.sh` on macOS or Linux or
`powershell -File scripts/install.ps1` on Windows. The installers use the
latest stable GitHub Release by default. Set `MY_SKILLS_VERSION` to the exact
tag to install a specific release, including a prerelease. Use an explicit
Rust tag while the latest stable release still serves Python 0.4.x.

For installer smoke tests, set `MY_SKILLS_RELEASE_BASE_URL` to the asset
directory itself, such as a `file://` URL for an unpacked `rust-release`
workflow artifact. The override takes precedence over release tag selection.
Set `MY_SKILLS_INSTALL_DIR` to a temporary directory. Regular installs default
to `$HOME/.local/bin` on Unix and `$env:LOCALAPPDATA\Programs\my-skills\bin`
on Windows. Pass `-AddToPath` on Windows to add the directory to the user and
current PowerShell session PATH.
