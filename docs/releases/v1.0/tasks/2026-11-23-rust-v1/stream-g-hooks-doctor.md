# Stream G: git hooks and doctor

Read `README.md`, then `phase2-README.md`. Spec: §6.7, §6.8, D2, D4, Q2.

## Files you own

- `crates/my-skills/src/hooks.rs` (library)
- `crates/my-skills/src/commands/hooks.rs`, `crates/my-skills/src/commands/doctor.rs`
- `crates/my-skills/tests/hooks_*.rs`, `doctor_*.rs`, `support_g.rs`

## hooks

1. Target: the git repository of the active registry (`runtime::resolve_root`).
   Find the hooks directory with `git rev-parse --git-path hooks` run in the
   registry, which honors `core.hooksPath`. If the registry is not a git
   repository, print an error and exit 1.
2. `hooks install [--dry-run]` writes `pre-commit`, `post-merge`, `post-rewrite`,
   and `post-checkout`.
   - Each file is POSIX sh with `# managed-by: my-skills hooks v1` on line 2.
   - It finds `my-skills` with `command -v my-skills`. If that fails, it uses the
     absolute path of the current executable (`std::env::current_exe`),
     written into the script at install time.
   - It then runs `exec "$MY_SKILLS" hooks run <hook> "$@"`.
   - On Unix, set the mode to 0755.
   - A hook file without the marker is never overwritten. Print the one line to
     add to it, and exit 1 with nothing written.
   - Re-installing over our own marker replaces the file.
3. `hooks uninstall` removes only files that carry the marker.
4. `hooks status` prints a table and is also reused by doctor. For each hook it
   shows `installed` / `missing` / `foreign`, the marker version, and whether
   `my-skills` is reachable on PATH.
5. `hooks run pre-commit` (Q2 decision):
   - List the staged paths with
     `git diff --cached --name-only --diff-filter=ACMR -z`.
   - Collect the skill names under `<skills_root>/<name>/`.
   - For each skill, list its staged files (`git ls-files -z --stage -- <dir>`)
     and export the **index** versions into a temp dir with
     `git checkout-index --prefix=<tmp>/ -z --stdin`.
   - Run `validate::compose_validation` and `validate::validate_skill_for_host`
     on the export for each enabled target in the skill's hosts.
   - If `my-skills.toml` is staged, export it too and load it from the temp
     root to validate it.
   - Print each failure as `[BLOCKED] <skill>: <error>`, then exit 1 when
     anything failed. Exit 0 otherwise.
   - A skill whose files were only deleted is skipped.
6. `hooks run post-merge | post-rewrite | post-checkout`:
   - Run `commands::connect::sync` with `SyncArgs { yes: true, quiet: true,
     ..default }`. The integrator implements that function. Until it is
     merged, test only that the hook exits 0.
   - `post-checkout` runs only when its third argument is `1` (a branch
     checkout).
   - These hooks **always exit 0**, because git cannot undo the operation;
     print problems instead.
7. Tests use real `git` in temp repos: `git init`, commit, stage, and run the
   hook binary path. Cover: marker handling, a foreign hook left alone,
   staged-only validation (a broken working-tree file that is not staged must
   not block, and a broken staged file must block), `core.hooksPath`, and
   uninstall leaving foreign hooks alone.

## doctor

1. Port v0.4.1 `cmd_doctor` output line by line (`src/my_skills/inspection_commands.py`).
   For the `Update:` line, call `self_update::update_status_line(args.no_update_check)`;
   stream I implements it.
2. Add the v1 sections (spec §6.8):
   - **Link capability**: create and remove a link in a temp dir with
     `link::create_link` / `link::remove_link`. Report `symlink`, `junction`,
     or `copy fallback: <reason>`.
   - **Registry git**: the current branch versus the default branch (from
     `origin/HEAD` if set, else `main`), rebase or merge in progress (from
     `.git/rebase-merge`, `.git/rebase-apply`, `.git/MERGE_HEAD`), the
     uncommitted change count, and ahead/behind versus upstream. Print a
     warning line for each problem.
   - **Hooks**: the status from section 4 above.
   - **Connections**: counts from state by mode, for example
     `link 41 / copy 2 (fallback 0)`, and a count per status (`plan::status_of`).
   - **Full validation**: run `compose_validation` over every enabled skill and
     list the failures. This catches problems the staged-only pre-commit hook
     misses.
3. `--json` emits the same information as an object. Document its keys in your
   report.
4. The exit code is 0 unless the manifest is invalid, as in v0.4.1. Warnings do
   not fail.

## Done when

All checks in README rule 8 pass, every item above has a test, and the report
lists the doctor JSON keys and any v0.4.1 output differences.
