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
```

For local release verification from a working tree, use:

```bash
uv run my-skills bootstrap --dry-run
uv run my-skills doctor
uv run my-skills skills --json
uv run my-skills install my-skills --host hermes --dry-run
```

## 3. Required Verification

Run:

```bash
uv run pytest
uv build
uv run my-skills doctor
uv run my-skills skills --json
uv run my-skills bootstrap --dry-run
uv run my-skills install my-skills --host hermes --dry-run
```

Then verify the GitHub install path from the same clean clone:

```bash
uv tool install --force git+https://github.com/Seosiju/my-skills.git
my-skills bootstrap --dry-run
my-skills doctor
my-skills skills --json
my-skills install my-skills --host hermes --dry-run
```

The GitHub Actions workflow must pass the same test, build, source smoke, and
tool-install smoke checks.

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
