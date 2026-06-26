# my-skills

Personal cross-agent **Agent Skill registry**. Author a skill once, keep it in
one canonical place, and (in later phases) install and sync it across Claude
Code, Codex, Gemini CLI, and Hermes.

The canonical format is the [Agent Skills](https://agentskills.io/specification)
standard: each skill is a directory under `skills/<name>/` with a `SKILL.md`
that carries YAML frontmatter (`name`, `description`).

## Status

**Phase 2 — Safe install lifecycle.** On top of the Phase 1 core (manifest,
canonical `skills/`, frontmatter parser, Agent Skills validation, security
scan, host registry, `validate` / `doctor`), this adds copy-mode `install`,
`status`, and `uninstall` with content hashing and machine-local install state.

The dedicated `sync` / `sync --check` command and `--mode link` are Phase 3 / 6
and not implemented yet — see `docs/` for the full plan.

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

# Remove a managed install (state-recorded destinations only).
uv run my-skills uninstall email-drafting --host claude
```

`validate` and `install` exit non-zero on errors/blocks, so they work in CI.

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
