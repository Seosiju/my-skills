---
name: my-skills
description: Manage your my-skills registry from an agent conversation. Use when setting up or creating a registry, listing available skills, sharing a host-local skill into the registry, enabling or disabling a skill, or installing/syncing skills into Claude Code, Codex, Hermes, or Antigravity CLI (agy).
---

# My Skills Registry

Use the `my-skills` CLI as the source of truth. Do not edit host skill
directories directly when a CLI command can plan, share, install, sync, enable,
or disable the skill.

**Mental model.** Your registry is a folder (default `~/my-agent-skills`) holding
`my-skills.toml` (the manifest) and `skills/<name>/SKILL.md` (the canonical
originals). Host directories (`~/.claude/skills`, `~/.agents/skills`,
`~/.hermes/skills`, `~/.gemini/config/skills`) hold the host connections.
New registries use links by default, and edits to canonical skills are visible
through those links. In copy mode, the CLI detects local changes before
replacing a copy. You can also set `install_mode = "link" | "copy"` per host
under `[targets.<host>]` in `my-skills.toml` or `my-skills.local.toml`
(precedence for new connections: `install --mode` > `[targets.<host>] install_mode` >
`[defaults] install_mode`; existing connections keep their recorded mode until
converted with `convert`). A registry can also work as a plain folder without Git.

## Which registry am I operating on?

Before changing skills, run:

```bash
my-skills doctor
```

Read the `Registry:` line. If it is not the registry the user expects, fix it
before writing:

```bash
my-skills set-root /path/to/registry
```

Use `MY_SKILLS_ROOT=/path/to/registry my-skills <cmd>` only for a one-command
override.

Do not point at a clone of the `my-skills` CLI project. The CLI source checkout
contains `my-skills.toml` and `skills/` as seed sources and test fixtures, but it
is not the user's registry. If there is already a different active root, running
a command inside a clone does not silently switch it; use `set-root` only when
the user explicitly wants to make a registry active.

## First-time setup (the front door)

If the Rust CLI is missing, install it from the tool repo with Git and stable
Rust. On Windows, use the MSVC toolchain and build tools.

```bash
git clone --branch dev https://github.com/Seosiju/my-skills.git
cd my-skills
cargo install --locked --path .
my-skills --version
```

Once v1 binaries are published, follow the release installer instructions in
the [tool repo README](https://github.com/Seosiju/my-skills/tree/dev#install-the-cli).

Create the registry from outside the tool repo. `init-registry` prompts for a location (default
`~/my-agent-skills`), seeds the public-safe default skills, and runs `git init`:

```bash
my-skills init-registry
```

- Pass a path to skip the prompt: `my-skills init-registry ~/my-agent-skills`.
- `--no-defaults` creates an empty registry (no seeded skills).
- `--no-git` skips git (local-only use; the whole system works without git).

`init-registry` records this registry as the active root, so later commands work
from any directory. Deploy the enabled skills into your agents:

```bash
my-skills install --dry-run   # preview host changes
my-skills install
```

Secrets and real account config belong under `my-skills data-path <skill>`, not
in git (public or private).

## When there is no registry yet

If a command fails with `my-skills.toml not found`, no registry has been created
on this machine. Do not point at a CLI clone — create a registry:

```bash
my-skills init-registry
```

As a temporary override, `export MY_SKILLS_ROOT=/path/to/registry` if one already
exists elsewhere (for example on another machine you cloned).

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
my-skills share --from <claude|codex|hermes|agy> --plan --json
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

## Create Or Import A Skill

For a new skill, write it in a temporary directory first, then import it into
the registry:

```bash
my-skills import /path/to/skill
my-skills enable <skill>
my-skills install <skill> --host <host>
```

`import` copies the directory into canonical `skills/` and registers it in
`my-skills.toml`. New imports are disabled by default. Use `--enable` when the
user explicitly wants to register the skill as enabled immediately:

```bash
my-skills import /path/to/skill --enable
my-skills install <skill> --host <host>
```

Import sources must not contain symlinks. Copy real files into the source
directory before importing so audit and copy inspect the same tree.

After import, edit the canonical skill. Linked installs see those edits
immediately. Use `sync` to update managed copies.

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

Before writing into host directories, show the dry-run plan for the command
you intend to run.

```bash
my-skills install <skill> --host <host> --dry-run --json
my-skills sync <skill> --host <host> --dry-run
```

For `install`, review both `actions` and `audit`. For `sync`, review removals
as well as new and updated connections. If audit would block, surface the
finding. When the plan is acceptable, run the corresponding command.

```bash
my-skills install <skill> --host <host>
my-skills sync <skill> --host <host>
```

Multi-host writes require `--yes` after a reviewed dry-run plan. Read-only
checks such as `install --dry-run` and `sync --check` do not need `--yes`.
Only use `--host all --yes` when the user's request explicitly names all hosts
and the dry-run plan has been shown or otherwise reviewed.

## Antigravity CLI (agy)

AGY is a supported, opt-in host. Set `enabled = true` under `[targets.agy]`
and include `agy` in the desired skill's `hosts` list to add it to default
cross-host operations. An omitted target is disabled; an explicit `--host agy`
still selects it, following the same explicit-host rules as other agents.

The user-scope path is `~/.gemini/config/skills`; the project path is
`.agents/skills`. Copy and symlink discovery were verified on macOS with
Antigravity CLI 1.2.16. On Windows, `my-skills` links with junctions by default,
which Antigravity CLI 1.5.14 did not discover; Windows users can set
`[targets.agy] install_mode = "copy"` in `my-skills.local.toml` so `agy` uses
copies while other hosts keep links (`convert --to link --host all` will skip
`agy` automatically, while `--host agy` still converts as requested). This does
not establish Antigravity IDE or other-version compatibility. After installation,
start a new CLI session and use `/skills` to check discovery (on Windows, run
`agy -p '/skills' --output-format json` from PowerShell rather than Git Bash,
which rewrites `/skills` into a Windows path):

```bash
my-skills install <skill> --host agy --dry-run --json
my-skills skills --host agy --json
agy -p '/skills' --output-format json
```

Preserve unmanaged AGY skills: compare and back them up outside its discovery
path before migration. Do not force replacement or bypass audit automatically.

## Enable Or Disable

Use manifest toggles instead of editing TOML by hand, then validate:

```bash
my-skills enable <skill>
my-skills disable <skill>
my-skills validate <skill>
```

A later `sync` can prune managed connections for disabled skills. Preview
those removals with `sync --dry-run`.

## Other Commands

This skill covers the common flows. For the full command surface — including
`status`, `validate`, `import`, `data-path`, `uninstall`, and `doctor` — run:

```bash
my-skills --help
```

On a new machine, run `bootstrap` from a clone of the registry to reconcile
its host connections. Run `my-skills hooks install` separately for Git hooks.
