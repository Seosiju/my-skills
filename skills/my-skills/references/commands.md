# Commands and JSON Results

## Inspect and validate

- `my-skills skills [name...] --json` includes `canonical_path`, per-host
  `status`, and `connections` with `mode`, `link_kind`, and `fallback_reason`.
  Use `--brief` for compact rows without `connections`. Names intersect with
  `--group`, `--host`, and `--enabled` or `--disabled`.
- `my-skills validate [name...] --json` reports `ok` and per-skill `errors`
  and `warnings`. With no names it scans skill directories. Use `--host <host>`
  to check display for a specific service. Exit 1 means a failed check.
- `my-skills diff <name> --host <host> --patch --json` compares a connection
  to its canonical source. Exit 0 means a match and 1 means a difference
  or missing connection.
- Save SKILL.md in UTF-8 without a BOM. Use the directory name (lowercase
  letters, digits, single hyphens, up to 64 characters) for `name` and a
  description of up to 1,024 characters in YAML frontmatter.

## Reconcile with sync

```bash
my-skills sync <name> --host <host> --dry-run --json
my-skills sync <name> --host <host> --json
```

Inspect `actions`, `validation`, `blocked`, `applicable`, and `block_reasons`.
Dry-run can return 0 for a blocked plan. The applied response adds `applied`,
`summary`, `cleanup_warnings`, and per-action `result`. Reasons include
`block_drift`, `block_foreign`, `shared_destination`, `conflict`,
`inactive_registry`, and `needs_yes`. A successful no-op has `applied: true`.

- Sync reconciles enabled skills and prunes obsolete managed connections.
  Use `--no-prune` to keep obsolete connections or `--all` to include disabled skills.
- `--mode link|copy` sets the mode for new connections and requests compatible
  transitions for this run. See [Host modes](hosts.md) for the automatic rules.
  Scope `--mode link` or `--mode copy` with `--host <host>` because it applies to every selected host, including Windows agy configured for copy.
- `my-skills sync --check` detects drift, including pending transitions and
  removals, with a nonzero exit. It runs without writes and cannot use `--json`.
- A multi-host write requires `--yes` after the user approves the exact plan.
  Apply a scoped plan by dropping `--dry-run` and keeping the same selection.

## Create and import

`my-skills new <name> [--group <group>] [--host <host>]` registers a disabled
draft. Use `--description <text>` to set its description and `--dry-run --json`
to preview it. Edit the canonical file, validate it, and enable it for delivery.

`my-skills import <directory> --dry-run --json` previews an external skill
folder. The source must contain real files. Use `--enable` to opt in to
delivery. New imports default to disabled.

```bash
my-skills import --from <host> --plan --json
my-skills import --from <host> <name> --enable --dry-run --json
```

The first command lists candidates. With no name, `--dry-run` also lists them.
For a named skill, choose `--enable` or `--disable`. Review warnings and
conflicts before dropping `--dry-run`. Use `--force` only for an explicitly
approved overwrite. Success JSON includes `canonical_path`, `enabled`, and
`adopted_host`. Blocked imports return exit 1 with a reason.

## Usage and configuration errors

Exit 2 with `--json` returns `error: {code, message, hint}` for usage and
configuration errors. Codes include `usage`, `inactive_registry`, `unknown_skill`,
`unknown_group`, `unknown_host`, `already_exists`, `invalid_name`, `state_busy`,
and `config`. Sync confirmation refusals can return the normal plan report.
