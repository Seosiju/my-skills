# JSON Results, Dry Runs, And Multi-Host Rules

## Errors

When a `--json` command hits a usage or configuration error, read the
`error` object on stdout and expect exit `2`. The human diagnostic is on stderr.

```json
{"error":{"code":"unknown_host","message":"unknown host: example","hint":null}}
```

Codes are `usage`, `inactive_registry`, `unknown_skill`, `unknown_group`,
`unknown_host`, `already_exists`, `invalid_name`, `state_busy` (writer lock
timed out after 30s), and `config`. `hint` is a string or `null`. A blocked
validation or dry-run plan uses its normal report shape.

## Creating skills (`new --json`)

`my-skills new <name> [--group <group>] [--host <host>] [--description <text>] [--dry-run] [--json]`
creates `skills/<name>/SKILL.md` (outside any host directory) and registers it
disabled (`enabled = false`). It keeps long descriptions in full and warns
when a selected service would shorten them. The selection is the explicit
`--host` list or, by default, the enabled targets. JSON returns `skill`, `enabled`, `group`, `hosts`,
`canonical_dir`, `canonical_path`, `created`, `next`, and `registry` (plus
`warnings` when present and `dry_run: true` only with `--dry-run`). Exit `2` emits `invalid_name`,
`already_exists`, `unknown_host`, `inactive_registry`, `usage` (including
`--description` errors), or `config`.

## Resolving names and paths (`resolve`)

Run `my-skills resolve <target> --json` to map a registered skill name,
canonical path, or host path (including disabled hosts and files inside a skill)
to its canonical path without hashing or updating state or the root cache.
Targets that are absolute, start with `.` or `~`, or contain `/` (or `\` on
Windows) are paths; all others are skill names.

- **Exit `0`:** `state` is `registered` or `unregistered` (real draft dir under
  `skills/`, or an owned host record removed from `my-skills.toml` whose
  canonical dir still exists).
- **Exit `1`:** `state` is `foreign`, `unmanaged`, `ambiguous`, or `unsafe`
  (`..` traversal, escaping internal link, or redirected host link).
- **Exit `2`:** unknown skill name (`unknown_skill`) or usage/config error.

Read `canonical_dir`, `canonical_path`, `matched_host`, `mode`, `reason`,
`write_refusal`, and `registry_agents_md` (all `null` when inapplicable).

## Validation

Use `my-skills validate <skill> --json` or
`my-skills validate --group <group> --json` before applying a skill. The
result has `registry: {path, source}`, overall `ok`, and `skills` entries with
`skill`, `ok`, `errors`, and `warnings`. Exit `0` means the checks passed, `1`
means a check failed, and `2` means a usage or configuration error.

Content diagnostics are warnings, including skills a service hides and
names or descriptions it displays differently. `install` and `sync` deliver
the skill as written. A missing `SKILL.md` fails validation. Pass `--host <host>` to preview that service's warnings even
when it is disabled or outside the skill's host list. A bare
`validate` scans all skill directories, including unregistered ones.

For consistent display across services, save `SKILL.md` in UTF-8 without a BOM.
Start with a YAML block between `---` lines. Use the folder name for `name`,
with lowercase letters, digits, and single hyphens, up to 64 characters.
Include a description of up to 1,024 characters. Quote values that contain
YAML syntax such as ` #`.

## Dry-run and applied JSON (`install` and `sync`)

A preview is always `--dry-run --json`; to apply, drop only `--dry-run`:

```bash
my-skills install <skill> --host <host> --dry-run --json   # preview
my-skills install <skill> --host <host> --json             # apply
my-skills sync <skill> --host <host> --dry-run --json      # preview
my-skills sync <skill> --host <host> --json                # apply
```

Shared fields in both preview (`dry_run: true`) and applied (`dry_run: false`)
JSON:

- `registry` gives `{path, source}`. The source is `flag` for `--root`,
  `env` for `MY_SKILLS_ROOT`, `cwd` for discovery, or `cache` for the saved root.
- `actions`: planned connection changes (for `sync`, review removals as well as
  new and updated connections).
- `validation`: per-skill and per-host validation checks.
- `blocked`: `true` if validation, a blocked plan item, or an inactive
  cwd registry would refuse the write (`needs_yes` alone leaves `blocked: false`).
- `applicable`: `true` only if the write can proceed without blocking or missing
  `--yes`.
- `block_reasons`: structured reasons explaining every refusal, including `block_drift`, `block_foreign`, `shared_destination` (when a
  single-host operation would alter a destination shared with an unselected
  recorded peer), `needs_yes`, `inactive_registry`, and late `conflict`. Host
  rows in `validation` that merely repeat skill-level errors or warnings are
  omitted.

Applied JSON (`--json` without `--dry-run`) adds:

- `applied`: `true` when every write succeeded and no action was `blocked` or
  `failed` (no-op syncs and post-commit cleanup warnings still return `true`).
- `summary`: `{created, updated, removed, unchanged, blocked, failed}` (excludes
  `skipped` rows).
- `cleanup_warnings`: `[{skill, host, path, message}]`.
- Per-action `result` (`created`, `updated`, `removed`, `unchanged`, `blocked`,
  `failed`, `skipped`), `requested_mode`, `mode` (actual mode after fallback, or
  `null` when skipped/blocked/failed before completion), `fallback_reason`, and
  `error`.

Exit-code rules:

- Both `install --dry-run` and `sync --dry-run` exit `1` **only** on validation
  failure and exit `0` otherwise — even when `blocked: true` or
  `applicable: false`.
- Without `--dry-run`, `install --json` and `sync --json` exit `0` when applied,
  `1` when blocked or failed, and `2` on usage, configuration, `inactive_registry`,
  or `needs_yes` refusals (post-plan refusals still emit the applied JSON
  document; early errors such as `unknown_skill`, `state_busy`, and
  `sync --json --check` emit `{"error": ...}`).
- Use `sync --check` when you need a non-zero exit code for any drift (drift
  only).
- Explicit `sync <skill...>` or `sync --group <group>` validates all
  wanted skills on the selected hosts, including `FRESH` links (failing `1` if a
  wanted skill is broken). Disabled skills whose only planned action is `REMOVE`
  (prune) skip those checks unless `--all` puts them in scope to connect.
- `sync --check` takes precedence over `--dry-run` and cannot be combined with
  `--json`.

## Compact catalog and status filters

Use `my-skills skills [skill...] --json --brief` for compact rows. Positional
`skill...` names intersect (AND) with `--group`, `--host`, and
`--enabled`/`--disabled`. A row keeps `error` when skill metadata is missing or
invalid. Add `--with-governance` to put `provenance` under
`governance` in each brief row.

Use `my-skills status [skill...] --group <group> --json` for a top-level JSON
array filtered by the union of names and groups; explicitly selected disabled
skills keep their normal per-host labels (such as `MISSING`). Unknown skill names
exit `2` (`unknown_skill`).

## Comparing installed content

Use `my-skills diff <skill> --host <host> --patch --json` for drift or conflict.
An explicit host is checked even if it is disabled or the manifest does not
want the connection. Diff uses the install record's destination if one exists.
With no record, an existing entry is `UNMANAGED` and an absent entry is
`MISSING`.

Diff lists live file changes or reports `link_target` (the target's display
path) and `link_matches` (whether the link points to the canonical skill).
Both link fields are `null` for other entries. A `FOREIGN` entry counts as
different even if the live content matches. Identical edits to an owned copy
and its canonical skill can show `CONFLICT` with no live differences. Exit
`0` means a match, `1` a difference or missing entry, and `2` an error.

## Host scope and `--yes` confirmation

- Default to the current host inside a specific agent. Use `--host all` only
  when the user explicitly asks for every host or cross-agent installation.
- A status table showing stale or missing entries across multiple hosts is
  informational only and is not permission to update every host. If the user
  says "update" without saying "all hosts", "every host", or "cross-agent",
  update only the current host.
- Multi-host writes require `--yes` after a reviewed dry-run plan. Without
  `--yes`, dry-run JSON reports `applicable: false` with a `needs_yes` reason.
- Never add `--yes` yourself to a multi-host write. `--yes` means the user
  already approved that exact multi-host plan. If the user has not explicitly
  approved all hosts, stop after the dry-run and ask which host to update.
- Read-only checks such as `install --dry-run`, `sync --dry-run`, and
  `sync --check` do not need `--yes`.
- Only use `--host all --yes` when the user's request explicitly names all hosts
  and the dry-run plan has been shown or otherwise reviewed.
