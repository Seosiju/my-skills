# my-skills

Personal cross-agent **Agent Skill registry**. Author a skill once, keep it in
one canonical place, and (in later phases) install and sync it across Claude
Code, Codex, Gemini CLI, and Hermes.

The canonical format is the [Agent Skills](https://agentskills.io/specification)
standard: each skill is a directory under `skills/<name>/` with a `SKILL.md`
that carries YAML frontmatter (`name`, `description`).

## Status

**Phase 1 — Portable skill core.** Implemented: the manifest, canonical
`skills/`, a frontmatter parser, Agent Skills validation, a static security
scan, the host registry, and the `validate` / `doctor` CLI commands.

Install and sync (Phase 2+) are not implemented yet — see `docs/` for the full
plan.

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
```

`validate` exits non-zero if any skill has errors, so it works in CI.

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
