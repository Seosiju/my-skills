# my-skills

**Write an Agent Skill once. Use it in every agent.**

_One canonical home for your skills — installed, synced, and shared across Claude Code, Codex, and Hermes._

**English** | [한국어](README.ko.md)

[![Python](https://img.shields.io/badge/python-3.11+-blue.svg)](https://www.python.org/)
[![built with uv](https://img.shields.io/badge/built%20with-uv-purple.svg)](https://docs.astral.sh/uv/)
[![spec: Agent Skills](https://img.shields.io/badge/spec-Agent%20Skills-green.svg)](https://agentskills.io/specification)
[![License: MIT](https://img.shields.io/badge/license-MIT-yellow.svg)](LICENSE)

`my-skills` is a small CLI that keeps your [Agent Skills](https://agentskills.io/specification)
in a single place and installs the same skill into every AI coding agent you use.
Edit a skill once; `sync` propagates it everywhere — and tells you when a copy has
drifted instead of silently clobbering your work.

## Why

- **Author once, run anywhere** — the same skill works in Claude Code, Codex, and Hermes.
- **No silent overwrites** — installs copy by default and detect local edits (drift) before touching them.
- **Machine-local stays local** — secrets, paths, and accounts never land in a canonical skill or in git.
- **CI-friendly** — `validate`, `install`, and `sync --check` exit non-zero on errors or drift.

## How it works

A skill is just a directory under `skills/<name>/` with a `SKILL.md` that carries
YAML frontmatter (`name`, `description`) — the [Agent Skills](https://agentskills.io/specification)
standard. The `skills/` directory is the **canonical** source of truth.

Each agent (Claude Code, Codex, Hermes) is a **host**. `install` copies a canonical
skill into a host; `sync` keeps those copies up to date. Because copies can be edited
in place, `my-skills` tracks **drift** so a `sync` never overwrites local changes
without telling you.

## Quick start

Requirements: **Python 3.11+** and [uv](https://docs.astral.sh/uv/).

```bash
git clone https://github.com/Seosiju/my-skills.git
cd my-skills

uv run my-skills doctor     # check environment, detected hosts, target paths
uv run my-skills skills     # list skills + per-host install status
uv run my-skills install    # install enabled skills into your agents
```

That's it — your skills are now available in every detected agent.

`skills` shows every skill and where it's installed across your hosts:

```text
SKILL             ENABLED  CLAUDE   CODEX    HERMES
----------------  -------  -------  -------  -------
cli-inventory     yes      fresh    fresh    missing
personal-profile  yes      fresh    stale    missing
my-skills         yes      fresh    fresh    fresh
```

## Included skills

| Skill | What it does |
|-------|--------------|
| `cli-inventory` | Discover the CLI tools installed on this machine (PATH + Homebrew/npm/pipx/cargo/gem/pip) and record them to a machine-local inventory you can read back quickly. |
| `personal-profile` | Remembers durable user facts (identity, preferences) and applies them across agents. |
| `my-skills` | Agent-facing skill that guides catalog, share, install, and sync workflows through the CLI. |

## Everyday commands

```bash
# See what exists and where it's installed.
uv run my-skills skills              # add --json for agents/UIs
uv run my-skills status              # install status per skill and host

# Install / update.
uv run my-skills install --dry-run   # preview the plan, write nothing
uv run my-skills install             # enabled skills -> enabled hosts
uv run my-skills install cli-inventory --host claude
uv run my-skills sync                # push canonical edits to managed installs
uv run my-skills sync --check        # detect drift only (non-zero exit if not fresh)

# Remove a managed install (recorded destinations only).
uv run my-skills uninstall cli-inventory --host claude

# Turn a skill on/off for default install/sync selection.
uv run my-skills enable cli-inventory
uv run my-skills disable cli-inventory
```

### Bring in a skill you already wrote

```bash
# Import an external skill directory into canonical skills/.
uv run my-skills import ~/.hermes/skills/cli-inventory

# Or review a host's local skills, then promote one into my-skills.
uv run my-skills share --from claude --plan --json
uv run my-skills share --from claude cli-inventory --enable
uv run my-skills sync cli-inventory
```

### Develop a skill live

```bash
# Symlink the host copy to canonical so edits show up without a sync.
uv run my-skills install cli-inventory --host claude --mode link
```

A linked install always reports `FRESH`; `uninstall` removes only the symlink and
never the canonical source. Copy mode is the default.

## How it stays safe

- **Copy by default.** Installs copy the canonical directory; nothing changes until the next `install` or `sync`.
- **Collision = block.** A pre-existing, unmanaged destination is never overwritten.
- **Drift-protected.** A locally edited copy is reported, not clobbered.
- **Atomic writes.** Installs stage to a temp dir and swap into place; failures roll back.

Install state is machine-local (`$XDG_STATE_HOME` or `~/.local/state/my-skills/`)
and never committed.

`sync` and `status` classify each (skill, host) so you always know what a write would do:

| State | Meaning |
|-------|---------|
| `FRESH` | install matches canonical |
| `STALE` | canonical changed; `sync` will update it |
| `DRIFTED` | the install was edited locally; `sync` won't touch it |
| `CONFLICT` | both sides changed — no auto-merge |
| `MISSING` | registered but not installed |
| `UNMANAGED` | a copy exists that my-skills did not install |

`sync` only writes the safe cases; `DRIFTED`, `CONFLICT`, and `UNMANAGED` block with a non-zero exit.

### Machine-local data

Canonical skills never store machine-specific data. Skills that need real local data
(e.g. the `personal-profile` memory) read and write a single shared data root instead:

```bash
uv run my-skills data-path personal-profile          # resolve the path
uv run my-skills data-path personal-profile --create # and create it
```

The data root is machine-local and never committed.

## Layout

```text
my-skills.toml            # manifest: hosts + skills + defaults
skills/<name>/SKILL.md    # canonical skills
src/my_skills/            # the CLI package
tests/                    # unit + fixture-driven tests
```

The manifest's `enabled` flag controls default selection: a bare `install` / `sync`
targets only `enabled = true` skills; pass `--all` to target every registered skill.

## Tests

```bash
uv run pytest
```

## License

[MIT](LICENSE)
