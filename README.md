# my-skills

**English** | [한국어](README.ko.md)

Personal cross-agent **Agent Skill registry**. Author a skill once, keep it in
one canonical place, and install, sync, or share it across Claude Code, Codex,
and Hermes.

The canonical format is the [Agent Skills](https://agentskills.io/specification)
standard: each skill is a directory under `skills/<name>/` with a `SKILL.md`
that carries YAML frontmatter (`name`, `description`).

## Status

**Phase 7 — catalog and sharing UX.** The registry now covers the portable skill
core, safe install/status/uninstall lifecycle, drift-aware `sync`, shared data
roots, `import`, development `--mode link`, the `skills` catalog (which now shows
per-host install status inline by default), and the `share` workflow for
promoting host-local skills into canonical `skills/`.

Deferred items such as watch mode, marketplace-style publish/export, upgrade
migration, and real Windows/WSL2 verification remain in `docs/` as future work.

### Available skills

| Skill | What it does |
|-------|--------------|
| `repo-analysis` | Host-neutral routine for orienting in an unfamiliar repository (purpose, layout, build/test commands, entry points). |
| `cli-inventory` | Declares the CLI tools a workflow requires and checks PATH availability via `scripts/check_tools.py`. The required-tool *policy* is committed; actual per-machine results stay machine-local. |
| `shared-agent-operation` | Baseline, host-neutral operating conventions shared across AI coding agents. |
| `personal-profile` | Memory-like skill: remembers durable user facts (identity, preferences) and applies them across agents. Canonical holds instructions + schema only; the profile data lives in the [shared data root](#shared-data-root), never committed. |
| `my-skills` | Agent-facing management skill that guides catalog, share, install, sync, enable, and disable workflows through the CLI. |

**Machine-local boundary.** Canonical skills never store machine-specific data
(hostnames, absolute paths, accounts, auth/versions). That data lives under a
git-ignored `local/` directory (e.g. `local/cli-inventory/`) — see
`skills/cli-inventory/references/required-tools.md`.

## Requirements

- Python 3.11+
- [uv](https://docs.astral.sh/uv/)

## Commands

```bash
# Validate every canonical skill (or a single one) against the standard.
uv run my-skills validate
uv run my-skills validate shared-agent-operation

# Report environment, detected hosts, target paths, and manifest health.
uv run my-skills doctor

# List registered canonical skills with per-host install status.
# Add --json for agent/UI consumption.
uv run my-skills skills
uv run my-skills skills --json

# Preview the install plan without writing anything, then install.
uv run my-skills install --dry-run
uv run my-skills install --dry-run --json
uv run my-skills install            # enabled skills -> enabled targets (copy)
uv run my-skills install email-drafting --host claude

# Show install status per skill and host.
uv run my-skills status

# Detect drift without changing anything (non-zero exit if anything is not FRESH).
uv run my-skills sync --check

# Propagate canonical edits to managed installs (create missing, update stale).
uv run my-skills sync

# Remove a managed install (state-recorded destinations only).
uv run my-skills uninstall email-drafting --host claude

# Print a skill's shared machine-local data directory (--create to mkdir it).
uv run my-skills data-path personal-profile
uv run my-skills data-path personal-profile --create

# Development install: symlink the host copy to canonical (edits reflect live).
uv run my-skills install repo-analysis --host claude --mode link

# Import an external skill directory into canonical skills/ (--force to overwrite).
uv run my-skills import ~/.hermes/skills/repo-analysis

# Review host-local skills before sharing them into canonical my-skills.
uv run my-skills share --from claude --plan --json

# Share one host-local skill, register it in the manifest, then sync it.
uv run my-skills share --from claude repo-analysis --enable
uv run my-skills share --from claude repo-analysis --disable
uv run my-skills sync repo-analysis

# Toggle manifest participation for default install/sync selection.
uv run my-skills enable repo-analysis
uv run my-skills disable repo-analysis
```

`validate`, `install`, and `sync` (incl. `--check`) exit non-zero on
errors/blocks/drift, so they work in CI.

### Shared data root

Canonical skills are *copied* to each host on `sync`, so a copy-relative
`local/` directory would diverge per host. Skills that hold real machine-local
data (for example a `personal-profile` memory) instead read and write a single
machine-level data root that every host shares:

```text
$XDG_DATA_HOME/my-skills/<skill>/        # POSIX (fallback ~/.local/share/...)
%LOCALAPPDATA%\my-skills\data\<skill>\   # Windows
```

`my-skills data-path <skill>` resolves that path so a `SKILL.md` never hardcodes
it. The data root is machine-local and never committed — it is the one
sanctioned exception to skill host-neutrality (the path belongs to `my-skills`,
not to any host). Pure-instruction skills (`repo-analysis`,
`shared-agent-operation`) do not need it.

### Development mode (`--mode link`)

`install --mode link` makes the host copy a **directory symlink** to the
canonical skill, so edits show up immediately without a `sync`. A linked install
always reports `FRESH`; if the link is replaced it reports `DRIFTED`, and if it
is deleted a re-install re-links it (never copies). `uninstall` removes only the
symlink — the canonical source it points at is never deleted. If the OS cannot
create a symlink, the command fails with an explicit message rather than
silently copying. Copy mode remains the default.

### Importing an existing skill

`import <path>` brings a skill you authored in some host into the canonical
`skills/`. It validates the source (standard + security scan), then copies it in
under its frontmatter `name`. An identical skill is a no-op; a *different*
existing skill is left untouched unless you pass `--force`. Import only writes to
`skills/` — afterwards add `[skills.<name>]` to `my-skills.toml` and run `sync`.

### Sharing a host-local skill

`share --from <host> --plan --json` scans a host skill directory and emits
deterministic candidate/risk JSON for agent conversations. Each candidate reports
the source path, canonical status, content hash, validation/security risks,
available choices, and the recommended default.

After reviewing the plan, apply one selected skill:

```bash
uv run my-skills share --from claude repo-analysis --enable
uv run my-skills share --from claude repo-analysis --disable
```

`share` validates the source, copies it into canonical `skills/`, registers
`[skills.<name>]` for all enabled targets, validates the canonical copy, and
adopts the source host's existing copy into local state so `sync <skill>` can
continue without treating that source host as an unmanaged collision. A different
canonical copy is never overwritten unless `--force` is passed explicitly.

### Agent management skill

The canonical `my-skills` skill is installed like any other skill:

```bash
uv run my-skills install my-skills --host all
```

In Claude Code it can be invoked as `/my-skills` when the host exposes skills as
slash commands. In Codex and Hermes, use the host's native skill invocation. The
skill does not modify files directly; it guides the agent to run `my-skills`
commands, show the user plan/risk output, and apply only the user's selected
choice.

### Drift states

`status` and `sync --check` classify each (skill, host):

| State | Meaning |
|-------|---------|
| `FRESH` | installed copy matches canonical and the recorded state |
| `STALE` | canonical changed; `sync` will update the install |
| `DRIFTED` | the installed copy was edited locally; `sync` won't clobber it |
| `CONFLICT` | both canonical and the installed copy changed — no auto-merge |
| `MISSING` | registered but not installed |
| `UNMANAGED` | a copy exists that my-skills did not install |
| `UNSUPPORTED` | the host is not in the skill's `hosts` list |

`sync` only writes the safe cases (create missing, update stale); `DRIFTED`,
`CONFLICT`, and `UNMANAGED` are reported and block with a non-zero exit.

### Safety model

- **Copy by default.** Installs copy the canonical directory; the installed
  copy does not change until the next `install` or `sync`. `--mode link` is an
  explicit development mode; if symlink creation fails, the command errors
  instead of silently falling back to copy.
- **Collision = block.** If a destination already exists and is *not* recorded
  as managed by my-skills, the install is blocked and nothing is overwritten.
- **Drift-protected.** If an installed copy was modified locally, install and
  uninstall refuse to clobber it; they report it instead.
- **Managed-only uninstall.** `uninstall` removes only destinations recorded in
  the local state file; unmanaged sibling files are never touched.
- **Atomic writes.** Installs stage into a temp directory and swap into place;
  a failure restores the previous copy. State is written atomically.

Install state is machine-local (under `$XDG_STATE_HOME` or
`~/.local/state/my-skills/`) and is never committed.

## Layout

```text
my-skills.toml            # manifest: targets + skills + defaults
skills/<name>/SKILL.md    # canonical skills
src/my_skills/            # package (config, frontmatter, validation, security, hosts, cli)
tests/                    # unit + fixture-driven tests
```

The manifest's `enabled` flag controls default selection: a bare
`install` / `sync` targets only skills with `enabled = true`; pass `--all` to
target every registered skill (see the plan, sections 5.5 / 9.5 / 9.6).

## Tests

```bash
uv run pytest
```
