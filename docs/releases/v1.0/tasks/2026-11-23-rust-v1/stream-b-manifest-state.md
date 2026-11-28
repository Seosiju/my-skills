# Stream B: manifest, state, data path

Read `README.md` in this folder first.

## Goal

Implement the contracts in `manifest.rs`, `state.rs`, and `data.rs` with v0.4.1
behavior from `src/my_skills/config.py`, `state.py`, `data.py`, plus the v1
changes from spec §5 D1 and §8.

## Files you own

- `crates/my-skills/src/manifest.rs`, `state.rs`, `data.rs`
- `crates/my-skills/tests/manifest_*.rs`, `state_*.rs`, `data_*.rs`
- `crates/my-skills/tests/fixtures/manifest/`, `tests/fixtures/state/`

## Manifest

1. `load_manifest(root, cli)` reads `root/my-skills.toml` (missing file is an error)
   and optional `root/my-skills.local.toml`. Merge each table key by key with
   CLI > local > main > built-in, exactly like `config.py`. That covers
   `[defaults]`, `[audit]`, each `[targets.<name>]`, each `[skills.<name>]`,
   and `skills_root`.
2. Targets: the union of the built-in hosts (`crate::hosts::ALL`) and every
   declared target, sorted by name. The path is the merged `path` or the host's
   `default_user_path`. If neither exists, return an error with the v0.4.1
   message. `enabled` defaults to the host's `default_enabled`, else true.
   `scope` defaults to `"user"`.
3. **v1 changes:**
   - `install_mode` defaults to `link`.
   - `"link"` and `"copy"` are the only valid values. Any other value is an
     `Error::Manifest` that names the file and the value.
   - `CliOverrides.install_mode` wins over both files.
4. Skills: `enabled` defaults to true, `hosts` is a string list (non-strings
   dropped, as in v0.4.1), `source_type` and `source_revision` are strings, and
   `group` is an optional string.
5. Lenient typing like v0.4.1 (`_as_str`, `_as_bool`, `_as_int`): wrong-typed
   values fall back to defaults, except `install_mode` (see 3).
6. `expand_path`: `~` uses `crate::paths::home_dir()`. Expand `$VAR` and `${VAR}`
   like `os.path.expandvars`, so an unknown variable stays as written. On
   Windows only, also expand `%VAR%`. Make relative results absolute against
   the current directory. Do not resolve links.
7. `selected_skills`: explicit names must all exist. Otherwise return
   `unknown skill(s): a, b` with the names sorted. `all` returns everything;
   neither returns only the enabled skills. Keep manifest order (sorted by name).

## State

1. `State::load(None)` uses `default_state_path()`. A missing file gives an
   empty state.
2. Errors with the same wording as `state.py`:
   - `schema_version` greater than 1
   - a record that is not an object, with its index
   - missing required fields, listing them sorted and with the index
3. Unknown fields are ignored.
4. `save()`:
   - Create parent directories.
   - Write to `state.json.tmp` in the same directory, then rename.
   - Output `{"schema_version": 1, "installs": [...]}` with records sorted by
     (skill, host), 2-space indent, and a trailing newline.
   - Field order is the struct order. `link_kind` and `fallback_reason` are
     omitted when `None`.
5. Compatibility test: put a synthetic v0.4.1-shaped `state.json` in fixtures
   with every field and both modes, and with no real paths. Load it, save it,
   and check that the result is semantically equal. Then check it is
   byte-identical to the input when the input uses Python's
   `json.dumps(indent=2)` formatting. Generate that input with
   `uv run python` from v0.4.1 `State.save()`.

## Data path

`skill_data_path(skill, create)` returns `paths::data_root()/<skill>`.

- Validate the name with `crate::validate::validate_name`. Stream D implements
  it; until then, mark the rejection tests
  `#[ignore = "needs stream D: validate_name"]`.
- With `create`, create the directory. Without it, write nothing.

## Done when

All checks in README rule 8 pass, and there are tests for:
- precedence across the three layers
- each lenient fallback
- the `install_mode` default and the invalid-value error
- unknown targets with and without a path
- `expand_path` (with `HOME` set to a temporary directory)
- every state error message
- the compatibility round trip
