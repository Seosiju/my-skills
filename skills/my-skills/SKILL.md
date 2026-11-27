---
name: my-skills
description: Manage a personal my-skills registry. Use to set up a registry, find or edit canonical skills, create or import skills, or manage connections to Claude Code, Codex, and Antigravity CLI (agy).
---

# My Skills Registry

Run `my-skills doctor --quick` and check `Registry:` before making changes.
A user's registry contains `my-skills.toml` and canonical files under
`skills/<name>/`. Never point the CLI at its own tool repo as your user registry.
Use `my-skills --root <path> <command>` for a one-command override or
`my-skills set-root <path>` when the user asks to switch.

Edit `skills/<name>/SKILL.md` and its supporting files in the registry. If you
need the path, read `canonical_path` from `my-skills skills <name> --json`.
Avoid editing host copies directly. A local edit makes a copy `DRIFTED`.
Keep account settings and secrets under `my-skills data-path <name>`.

## Everyday work

- List skills and host states with `my-skills skills [name...] --json`.
  Statuses are uppercase in JSON: `FRESH` (current), `STALE` (needs refresh), `DRIFTED` (locally edited), `CONFLICT` (both sides edited), `FOREIGN` (another registry), `MISSING` (not connected), and `UNSUPPORTED` (excluded; `-` in tables).
- Create a disabled draft with `my-skills new <name> [--host <host>]`, or use
  `my-skills import <directory>` for an existing skill.
- Find host-local candidates with `my-skills import --from <host> --plan --json`.
  Preview the selected import with `--enable` or `--disable` and `--dry-run`.
- Use `my-skills enable <name>` or `my-skills disable <name>` to change the
  manifest. Sync or let the Git hooks reconcile connections.
- Run `my-skills validate <name> --json` after editing. Report display warnings
  so the owner can decide whether to change the canonical content.
- Inspect a connection with `my-skills diff <name> --host <host> --patch`.

## Apply changes

Links show canonical edits immediately. Git registries use hooks to validate
commits and refresh host connections. Follow the registry owner's Git workflow.
For manual reconciliation, select the current or requested host and preview.

```bash
my-skills sync <name> --host <host> --dry-run --json
my-skills sync <name> --host <host> --json
```

Review the actions, including removals, and `applicable` before applying. If
blocked, inspect `block_reasons`. Use `--host all` for explicit cross-host
requests. Never add `--yes` to a multi-host write without the user's approval
of that specific plan.

If skills are missing after delivery, restart Claude Code or open a new
Antigravity CLI session and use `/skills`. Codex usually detects changes
automatically and can reload them on restart.

- [Registry setup and Git hooks](references/setup.md)
- [Commands and JSON reference](references/commands.md)
- [Host modes and agy troubleshooting](references/hosts.md)
