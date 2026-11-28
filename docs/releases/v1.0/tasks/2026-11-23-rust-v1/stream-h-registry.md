# Stream H: registry commands

Read `README.md`, then `phase2-README.md`.

## Files you own

- `crates/my-skills/src/manifest_edit.rs`, `catalog.rs`, `sharing.rs`, `seeds.rs`
- `crates/my-skills/src/commands/registry.rs`
- `crates/my-skills/tests/registry_*.rs`, `support_h.rs`

## Port, with v0.4.1 behavior and output

| v1 | v0.4.1 source |
|---|---|
| `manifest_edit.rs` | `manifest_edit.py`: line-based edits that keep comments and formatting |
| `enable`, `disable` | `registry_commands.py` |
| `skills [--json] [--host] [--enabled|--disabled] [--with-status]` | `inspection_commands.cmd_skills`, `catalog.py` (status via `plan::status_of`) |
| `import <source> [--force] [--enable] [--skip-audit]` | `registry_commands.cmd_import`, `sharing.py` |
| `share --from <host> [skill] [--plan] [--json] [--enable|--disable] [--force] [--skip-audit]` | `registry_commands.cmd_share`, `sharing.py`, `sharing_formatting.py` |
| `init-registry [path] [--with-defaults|--no-defaults] [--no-git]` | `init_registry_commands.py` |
| `seeds.rs` | the seed files listed in `pyproject.toml` `force-include`, embedded at build time (add `include_dir` in your Cargo block) |

## v1 rules

1. **init-registry writes `install_mode = "link"`** in the new manifest
   (decision D1). v0.4.1 writes `"copy"`. This is a deliberate difference.
   Everything else in the scaffold, including the git init and the initial
   commit, matches v0.4.1.
2. **import / share keep their safety rules.** A symlink inside or at the
   source is blocked. The audit gate runs unless `--skip-audit`. The copy into
   canonical never follows links. Use `crate::link::entry_kind` to classify
   entries.
3. **Never write into a host directory.** share reads host-local skills and
   writes only into the registry. After share or import, print the v0.4.1
   follow-up hint, with `sync` wording.
4. **The seed content stays unchanged.** If the `my-skills` seed SKILL.md
   describes the copy model, list the stale sentences in your report. Do not
   rewrite them; the integrator updates the docs.
5. Manifest edits are atomic: write a temp file in the same directory, then
   rename.

## Tests

- Port each relevant pinned expectation from `tests/test_cli_catalog.py`,
  `test_cli_import.py`, `test_import.py`, `test_share.py`, `test_manifest_edit.py`,
  `test_init_registry.py`, `test_init_registry_git.py`, and `test_defaults.py`.
- Golden comparison for manifest edits: run v0.4.1 `manifest_edit` functions
  on a set of fixture manifests (comments, odd spacing, missing sections) and
  compare the bytes.

## Done when

All checks in README rule 8 pass, each command has black-box tests, and the
report lists every output difference from v0.4.1.
