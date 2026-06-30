# Contributing

Thanks for helping improve `my-skills`. This project is a GitHub-first Python
CLI built with `uv`, `pytest`, and `hatchling`.

## Development Setup

Requirements:

- Python 3.11+
- `uv`

Clone the repository and run the test suite:

```bash
git clone https://github.com/Seosiju/my-skills.git
cd my-skills
uv run pytest
```

Use the CLI from the working tree while developing:

```bash
uv run my-skills doctor
uv run my-skills skills --json
uv run my-skills bootstrap --dry-run
uv run my-skills install my-skills --host hermes --dry-run
```

## Contribution Guidelines

- Keep canonical skills under `skills/<name>/`.
- Keep host install directories, state, accounts, tokens, and local memory out of
  git.
- Put machine-local overrides in `my-skills.local.toml`, `local/`, or the data
  directory returned by `my-skills data-path <skill>`.
- Prefer dry-run commands before commands that write to host directories.
- Keep changes small and covered by tests when behavior changes.
- Do not add runtime dependencies without opening an issue or PR discussion first.

## Before Opening a Pull Request

Run:

```bash
uv run pytest
uv build
uv run my-skills doctor
uv run my-skills skills --json
uv run my-skills install my-skills --host hermes --dry-run
```

For release-related changes, also check `docs/release-checklist.md`.
