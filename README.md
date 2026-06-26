# my-skills

Personal cross-agent **Agent Skill registry**. Author a skill once, keep it in
one canonical place, and (in later phases) install and sync it across Claude
Code, Codex, and Hermes.

The canonical format is the [Agent Skills](https://agentskills.io/specification)
standard: each skill is a directory under `skills/<name>/` with a `SKILL.md`
that carries YAML frontmatter (`name`, `description`).

## Status

**Phase 3 — Sync and drift.** On top of the Phase 1 core (manifest, canonical
`skills/`, validation, security scan, host registry, `validate` / `doctor`) and
the Phase 2 install lifecycle (`install`, `status`, `uninstall` with content
hashing and machine-local state), this adds `sync` and `sync --check` with
3-way drift/conflict classification.

`--mode link`, `import`, and watch mode are Phase 6 and not implemented yet —
see `docs/` for the full plan.

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

# Preview the install plan without writing anything, then install.
uv run my-skills install --dry-run
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
```

`validate`, `install`, and `sync` (incl. `--check`) exit non-zero on
errors/blocks/drift, so they work in CI.

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
  copy does not change until the next `install`. (`--mode link` is rejected for
  now — no silent fallback.)
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
