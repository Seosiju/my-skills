# Stream I: self-update, and the Python updater guard

Read `README.md`, then `phase2-README.md`. Spec §6.9, §10.4. Read the stream F
report (`reports/stream-f.md`), especially Open issues.

## Files you own

- `crates/my-skills/src/self_update.rs`, `crates/my-skills/src/commands/update.rs`
- `crates/my-skills/tests/update_*.rs`, `support_i.rs`
- **Python, for the guard only:** `src/my_skills/update_commands.py`,
  `src/my_skills/update_apply.py`, and their tests in `tests/test_update_*.py`

## Rust `update`

1. Release source: GitHub Releases of `Seosiju/my-skills`. The asset names and
   `SHA256SUMS` format are set by `.github/workflows/release-rust.yml` and
   `scripts/install.sh` (stream F). Reuse them exactly.
2. Channels:
   - `stable`: the newest non-prerelease `v1.*` release.
   - `main`: the newest release including prereleases.

   Explain the mapping from v0.4.1's `main` channel in the report.
3. `update --check`: print the current version, the latest version, and the
   channel. Exit 1 when an update is available, 0 when up to date, and 2 when
   the lookup failed, matching v0.4.1 `_check_exit_code`.
4. `update --dry-run`: print what would be downloaded and replaced. Write
   nothing.
5. `update`:
   - Download the asset for the current target and verify its SHA-256 against
     `SHA256SUMS`.
   - Replace the running executable atomically. On Unix, write a temp file next
     to it and rename it over. On Windows, rename the running exe to
     `.old` first, then move the new one in.
   - On any failure, the old binary keeps working.
   - Then run the new binary with `--version` and confirm it.
6. `self_update::update_status_line(no_check)`: keep the signature. Return the
   one-line text `doctor` shows. Never panic or block for long; use a short
   timeout.
7. HTTP: choose a small, maintained client, for example `ureq` with rustls, and
   add it in your Cargo block. Honor `MY_SKILLS_RELEASE_BASE_URL` and an API
   base override so tests run against a local fake server or `file://`
   fixtures. No test may touch the network.

## Python 0.4.x guard (the trap stream F found)

When the v0.4.x updater sees a latest stable tag with major version 1 or
higher, it must not run `uv tool install git+...@v1.x`. Instead it prints a
short migration message: the v1 binary installers (`scripts/install.sh` and
`install.ps1`) and the spec link. It leaves the current install untouched and
exits 0.
- `update --check` reports the newest compatible 0.4.x state, then adds one
  line saying v1 is available.
- Add Python tests for both paths (`uv run pytest`).
- Bump nothing yet. The integrator decides the 0.4.2 release.

## Done when

- All checks in README rule 8 pass, and `uv run pytest` passes.
- Rust tests cover check, dry-run, a successful update, a checksum mismatch,
  and a failed download (the old binary stays).
- The report states exactly what a v0.4.1 user and a v0.4.2 user see once
  `v1.0.0` exists.
