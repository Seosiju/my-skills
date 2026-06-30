---
name: my-skills
description: Manage the shared my-skills registry from an agent conversation. Use when listing available skills, sharing a host-local skill into my-skills, enabling or disabling a shared skill, or installing/syncing shared skills into Claude Code, Codex, or Hermes.
---

# My Skills Registry

Use the `my-skills` CLI as the source of truth. Do not edit host skill
directories directly when a CLI command can plan, share, install, sync, enable,
or disable the skill.

## Running the CLI (read this first)

On a fresh machine, bootstrap once from the cloned repo:

```bash
uv run my-skills bootstrap
```

After bootstrap, invoke the installed command directly:

```bash
my-skills <command> ...
```

Do not use `uv run my-skills` for normal agent workflows; it only works inside
the cloned repo and bypasses the installed command that users expect in their
shell.

The CLI needs to know where the project (the `my-skills.toml` manifest and
`skills/`) lives. It resolves the root in this order: `$MY_SKILLS_ROOT`, then the
current directory or any parent, then a path cached from a previous successful
run. `bootstrap` writes that cache, so later commands work from **any**
directory.

If a command fails with `my-skills.toml not found`, the root has never been
seen on this machine. Fix it once, then retry:

```bash
export MY_SKILLS_ROOT=/path/to/your/my-skills   # the clone's directory
```

If the `my-skills` command itself is not on `PATH`, run bootstrap from the clone.
Use `cd "$MY_SKILLS_ROOT" && uv run my-skills <command>` only as a temporary
fallback while repairing the installation.

## List

```bash
my-skills skills
```

Shows each skill, whether it is enabled, and its install status per host
(`fresh`, `stale`, `drifted`, `missing`, or `-` when the skill does not target
that host). Add `--json` when another agent or UI needs a structured list:

```bash
my-skills skills --json
```

## Share From A Host

First produce a read-only plan:

```bash
my-skills share --from <claude|codex|hermes> --plan --json
```

Show the user the candidate skills, validation/audit risks, canonical status,
and available choices. Continue only after the user chooses to enable, disable,
or skip. Apply the selected choice:

```bash
my-skills share --from <host> <skill> --enable
my-skills share --from <host> <skill> --disable
```

Use `--force` only when the plan reports a *different* canonical skill and the
user explicitly confirms overwriting it.

Never use `--skip-audit` for share/import unless the user explicitly accepts the
audit risk. `--force` only answers "may overwrite?"; it does not bypass audit.

## Audit

Run audit before a write when the user asks about risk, provenance, or whether a
skill is safe to install:

```bash
my-skills audit <skill> --json
my-skills audit --all --json
my-skills skills --json --with-status
```

Use `audit --all --json` before applying multiple skills so bundle-level and
cross-skill findings are visible.

If audit returns `blocked: true`, stop and show the finding. Do not install,
sync, share, or import that skill. The only bypass is `--skip-audit`, and that
requires explicit user approval for the exact command.

## Install Or Sync

Default to the current host when the user asks to install a skill from inside a
specific agent. Use `--host all` only when the user explicitly asks for every
host or cross-agent installation.

A status table showing stale or missing entries across multiple hosts is
informational only. It is not permission to update every host. If the user says
"update" without saying "all hosts", "every host", or "cross-agent", update only
the current host.

Never add `--yes` yourself to a multi-host write. `--yes` means the user already
approved that exact multi-host plan. If the user has not explicitly approved all
hosts, stop after the dry-run and ask which host to update.

Before writing into host directories, show the dry-run plan:

```bash
my-skills install <skill> --host <host> --dry-run --json
```

Review both `actions` and `audit`. If audit would block, do not continue. If the
plan is acceptable, run the matching command:

```bash
my-skills install <skill> --host <host>
my-skills sync <skill> --host <host>
```

Multi-host writes require `--yes` after a reviewed dry-run plan. Read-only
checks such as `install --dry-run` and `sync --check` do not need `--yes`.
Only use `--host all --yes` when the user's request explicitly names all hosts
and the dry-run plan has been shown or otherwise reviewed.

## Enable Or Disable

Use manifest toggles instead of editing TOML by hand, then validate:

```bash
my-skills enable <skill>
my-skills disable <skill>
my-skills validate <skill>
```

## Other Commands

This skill covers the common flows. For the full command surface — including
`status`, `validate`, `import`, `data-path`, `uninstall`, and `doctor` — run:

```bash
my-skills --help
```
