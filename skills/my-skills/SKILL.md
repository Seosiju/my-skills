---
name: my-skills
description: Manage the shared my-skills registry from an agent conversation. Use when listing available skills, sharing a host-local skill into my-skills, enabling or disabling a shared skill, or installing/syncing shared skills into Claude Code, Codex, or Hermes.
---

# My Skills Registry

Use the `my-skills` CLI as the source of truth. Do not edit host skill directories directly when a CLI command can plan, share, install, sync, enable, or disable the skill.

## List

Run:

```bash
uv run my-skills skills --with-status
```

Use JSON when another agent or UI needs a structured list:

```bash
uv run my-skills skills --json --with-status
```

## Share From A Host

First produce a read-only plan:

```bash
uv run my-skills share --from <claude|codex|hermes> --plan --json
```

Show the user the candidate skills, validation risks, canonical status, and available choices. Continue only after the user chooses `share-enable`, `share-disable`, or `skip`.

Apply the selected choice:

```bash
uv run my-skills share --from <host> <skill> --enable
uv run my-skills share --from <host> <skill> --disable
```

Use `--force` only when the plan reports a different canonical skill and the user explicitly confirms overwriting it.

## Install Or Sync

Before writing into host directories, show the dry-run plan:

```bash
uv run my-skills install <skill> --host <host|all> --dry-run --json
```

If the plan is acceptable, run the matching install or sync command:

```bash
uv run my-skills install <skill> --host <host|all>
uv run my-skills sync <skill>
```

## Enable Or Disable

Use manifest toggles instead of editing TOML by hand:

```bash
uv run my-skills enable <skill>
uv run my-skills disable <skill>
```

After changes, validate the affected skill:

```bash
uv run my-skills validate <skill>
```
