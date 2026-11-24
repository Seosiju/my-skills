---
name: my-skills
description: Manage your my-skills registry from an agent conversation. Use when setting up or creating a registry, listing available skills, sharing a host-local skill into the registry, enabling or disabling a skill, or installing/syncing skills into Claude Code, Codex, or Antigravity CLI (agy).
---

# My Skills Registry

Use the `my-skills` CLI as the source of truth. Do not edit host skill
directories directly when a CLI command can plan, share, install, sync, enable,
or disable the skill.

**Mental model.** Your registry is a folder (default `~/my-agent-skills`) holding
`my-skills.toml` and `skills/<name>/SKILL.md` (the canonical originals), and can
work as a plain folder without Git. Host directories (`~/.claude/skills`,
`~/.agents/skills`, `~/.gemini/config/skills`) hold links by default (edits are
visible immediately); copy mode detects local changes before replacing a copy.
Secrets and real account config belong under `my-skills data-path <skill>`, not
in Git (public or private).

## Which registry am I operating on?

Before changing skills, run `my-skills doctor --quick` and check `Registry:` (or
`registry: {path, source}` in `--json` outputs). Switch only when the user
explicitly asks to make a registry active:

```bash
my-skills set-root /path/to/registry
my-skills --root /path/to/registry <cmd>   # one-command override, never saved
```

Do not point at a clone of the `my-skills` CLI project — its `my-skills.toml`
and `skills/` are seed sources and test fixtures, not a user registry. Read-only
commands, `sync --check`, and `--dry-run` never save or change the active root.
When another registry is active, read-only commands run against a cwd registry
with a stderr note, while real writes (including `bootstrap` and
`hooks install`/`uninstall`) refuse with exit `2` until you run from the active
registry, switch with `set-root`, or select one for the command with `--root`.

## Finding the canonical file (0 or 1 calls)

- **Link hosts:** the skill path in the host folder already points to the
  canonical directory under `skills/<name>/`. Edit there directly (0 CLI calls).
- **Copy hosts, nested files, or when unsure:** run `resolve` once:

```bash
my-skills resolve <path-or-skill> --json
```

Read `canonical_path` (or `canonical_dir`), `state`, `matched_host`, `mode`,
and `write_refusal`. Exit `0` resolves `registered` skills and `unregistered`
drafts under `skills/`; exit `1` flags `foreign`, `unmanaged`, `ambiguous`, or
`unsafe` paths.

## Everyday commands

- **List / status:** `my-skills skills [skill...] --json --brief` (intersects
  names with `--group`, `--host`, `--enabled`/`--disabled`) and
  `my-skills status [skill...] --group <group> --json`.
  Per-host statuses: `fresh`, `stale`, `drifted`, `foreign`, `missing`, or `-`
  (host excluded). `foreign` means another registry owns that connection;
  `sync` leaves it alone when pruning and blocks writes with `BLOCK_FOREIGN`.
- **Enable / disable:** use manifest toggles instead of editing TOML by hand,
  then validate (`my-skills enable <skill>`, `my-skills disable <skill>`,
  `my-skills validate <skill>`). A later `sync` prunes disabled connections
  (preview removals with `sync --dry-run --json`).
- Pass several skill names or repeat `--group <group>` with `install`, `sync`,
  `uninstall`, `enable`, `disable`, `audit`, or `validate`.
- Inspect drift or conflicts with `my-skills diff <skill> --host <host> --patch`.
- **New skill:** `my-skills new <name> [--group <group>] [--host <host>]`
  creates a disabled draft in `skills/<name>/SKILL.md` and registers it under
  `my-skills.toml.edit-lock` (edit → `my-skills enable <name>` → validate/commit).
- **Editing loop:** edit canonical `skills/<name>/...`, validate
  (`my-skills validate <skill> --json`), then commit (`pre-commit` validates;
  `post-commit` refreshes copy hosts automatically) and push in a Git registry
  — zero extra `my-skills` calls after committing. In a non-Git registry, run
  one call: `my-skills sync <skill> --host <host> --json` (after previewing with
  `--dry-run --json`).
- **Session reload:** after adding or changing skills, start a new host session
  if needed — Claude Code picks up skills on restart, Codex detects changes
  automatically (restart if needed), and Antigravity CLI requires a new
  session and `/skills`.
- Run `my-skills --help` for the full command list.

## Safe write loop (install and sync)

Default to the current host inside a specific agent. A status table showing
stale or missing entries across hosts is informational only; "update" without
explicitly saying "all hosts", "every host", or "cross-agent" targets only the
current host. **Never add `--yes` yourself** to a multi-host write — `--yes`
means the user already approved that exact multi-host plan; otherwise stop after
the dry-run and ask which host to update.

1. Preview is always `--dry-run --json` (instead of parsing text):

```bash
my-skills install <skill> --host <host> --dry-run --json
my-skills sync <skill> --host <host> --dry-run --json
```

2. Read `registry`, `blocked`, `applicable`, `block_reasons`, `validation`,
   `audit`, and `actions` (for `sync`, review removals as well as new/updated
   connections). Stop and surface findings if `blocked` is `true` or audit would
   block.
3. To apply, drop only `--dry-run` when `applicable` is `true` (or only
   `needs_yes` remains for an already-approved multi-host scope), and check
   `applied`, `summary`, `cleanup_warnings`, and each action's `result`:

```bash
my-skills install <skill> --host <host> --json
my-skills sync <skill> --host <host> --json
```

## Never bypass audit

If audit returns `blocked: true`, stop and show the finding — do not install,
sync, share, or import that skill. The only bypass is `--skip-audit`, which
requires explicit user approval for the exact command. `--force` only answers
"may overwrite?" and never bypasses audit.

## References

Read a reference only when the task needs it:

- [references/setup.md](references/setup.md) — First-time CLI/registry setup,
  `my-skills.toml not found`, `init-registry` flags, `bootstrap`, and Git hooks.
- [references/json-and-dry-run.md](references/json-and-dry-run.md) — `resolve`,
  `new --json`, dry-run and applied `install`/`sync --json`, error codes,
  `sync --check`, and multi-host rules.
- [references/share-and-audit.md](references/share-and-audit.md) — Sharing
  host-local skills (`share`), creating/importing skills (`import`), and
  security and bundle audits (`audit`, `--with-governance`).
- [references/hosts.md](references/hosts.md) — `install_mode` (`link` vs `copy`)
  precedence, `doctor --quick`, `convert`, and Antigravity CLI (`agy`) notes.
