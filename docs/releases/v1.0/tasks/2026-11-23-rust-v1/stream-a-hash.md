# Stream A: directory hash

Read `README.md` in this folder first.

## Goal

Implement `crates/my-skills/src/hash.rs` so `hash_file` and `hash_directory`
are byte-for-byte identical to v0.4.1 `src/my_skills/hashing.py` (spec §8.5).
Existing copy records on users' machines keep their FRESH/STALE status only if
this matches exactly.

## Files you own

- `crates/my-skills/src/hash.rs`
- `crates/my-skills/tests/hash_golden.rs`
- `crates/my-skills/tests/golden/hash/` (golden data and the generator script)

## Requirements

1. Walk with `walkdir` without following links. Hash regular files only.
2. Skip entries where `crate::ignore::is_ignored(rel)` is true.
3. Order by relative path with `/` separators, compared as UTF-8 bytes. This
   equals Python's `str` ordering for valid Unicode; add a test with mixed
   Korean, uppercase, digits, and punctuation names to prove it.
4. Stream 64 KiB chunks. Do not read whole files into memory.
5. Errors use `Error::io(path, err)`. A non-UTF-8 file name is an error whose
   message names the path. Python would raise too; say so in the report.

## Golden data

Ignored names (`.DS_Store`, `__pycache__/`) are in the repo's `.gitignore`, so
the fixture trees cannot be committed as folders. Instead:

1. Write `tests/golden/hash/make_golden.py` (standard library only). It reads
   `cases.json`, a list of `{ "name": ..., "files": { "<rel path>": "<base64 bytes>" }, "dirs": ["<empty dir>", ...] }`,
   builds each tree in a temporary directory, and calls v0.4.1
   `my_skills.hashing.hash_directory`. It writes `golden.json` as
   `{ "<case name>": "sha256:..." }`. Run it from the repo root with
   `uv run python crates/my-skills/tests/golden/hash/make_golden.py`.
2. Commit `cases.json`, `golden.json`, and the script.
3. `hash_golden.rs` builds the same trees in `tempfile` directories and checks
   each hash.

Cases to cover at minimum:

- an empty directory
- a single `SKILL.md`
- nested folders with scripts and references
- ignored entries at the root and nested (`.DS_Store`, `.git/HEAD`,
  `__pycache__/x.pyc`, `.omc/state`)
- `.gitignore` (must not be ignored)
- non-ASCII file names (Korean)
- a binary file larger than 64 KiB
- an empty file
- empty subdirectories (they do not change the hash)
- the same content under two different root folder names (same hash)

## Also test

- A directory symlink inside the tree is not followed, on Unix only
  (`#[cfg(unix)]`). Python `rglob` + `is_file()` would follow it. Document this
  deliberate difference in the report. v0.4.1 `import` blocks symlinks, so real
  registries are not affected.
- `hash_file` returns lowercase hex with no prefix.

## Done when

All checks in README rule 8 pass, and the report lists every case and the result.
