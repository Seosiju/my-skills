# Dry-Run JSON, Exit Codes, And Multi-Host Rules

## Dry-run JSON fields and exit codes

Preview writes with `--dry-run --json` instead of parsing text:

```bash
my-skills install <skill> --host <host> --dry-run --json
my-skills sync <skill> --host <host> --dry-run --json
```

Read these fields from the plain UTF-8 JSON response:

- `registry`: `{path, source}` showing which registry was resolved.
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
