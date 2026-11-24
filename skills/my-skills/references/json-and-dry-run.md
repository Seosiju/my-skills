# JSON Results, Dry Runs, And Multi-Host Rules

## Errors

When a `--json` command hits a usage or configuration error, read the
`error` object on stdout and expect exit `2`. The human diagnostic is on stderr.

```json
{"error":{"code":"unknown_host","message":"unknown host: example","hint":null}}
```

Codes are `usage`, `inactive_registry`, `unknown_skill`, `unknown_group`,
`unknown_host`, and `config`. `hint` is a string or `null`. A blocked validation,
audit, or dry-run plan uses its normal report shape.

## Validation

Use `my-skills validate <skill> --json` or
`my-skills validate --group <group> --json` before applying a skill. The
result has `registry: {path, source}`, overall `ok`, and `skills` entries with
`skill`, `ok`, `errors`, and `warnings`. Exit `0` means the checks passed, `1`
means a check failed, and `2` means a usage or configuration error.

The pre-commit gate applies standard validation, the security floor, host
limits, and the manifest audit policy. An over-limit description fails.
`--host <host>` narrows host checks to that host. A bare `validate` scans all
skill directories, including unregistered ones.

## Dry-run JSON fields and exit codes

Preview writes with `--dry-run --json` instead of parsing text:

```bash
my-skills install <skill> --host <host> --dry-run --json
my-skills sync <skill> --host <host> --dry-run --json
```

Read these fields from the plain UTF-8 JSON response:

- `registry` gives `{path, source}`. The source is `flag` for `--root`,
  `env` for `MY_SKILLS_ROOT`, `cwd` for discovery, or `cache` for the saved root.
- `actions`: planned connection changes (for `sync`, review removals as well as
  new and updated connections).
- `validation`: per-skill and per-host validation checks.
- `audit`: audit gate outcome and findings.
- `blocked`: `true` if validation, audit, a blocked plan item, or an inactive
  cwd registry would refuse the write.
- `applicable`: `true` only if the same command without `--dry-run` would exit
  `0` (including the `--yes` requirement for multi-host writes).
- `block_reasons`: structured reasons explaining every refusal, including audit
  blocks, `block_drift`, `block_foreign`, `needs_yes`, and `inactive_registry`.

Exit-code rules:

- Both `install --dry-run` and `sync --dry-run` exit `1` **only** on validation
  failure and exit `0` otherwise — even when `blocked: true` or
  `applicable: false`.
- Use `sync --check` when you need a non-zero exit code for any drift.
- `sync --json` requires `--dry-run`.
- `sync --check` takes precedence over `--dry-run` and cannot be combined with
  `--json`.

## Compact catalog results

Use `my-skills skills --json --brief` for compact rows. A row keeps `error`
when skill metadata is missing or invalid. Add `--with-governance` to put
`audit` and `provenance` under `governance` in each brief row.

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
