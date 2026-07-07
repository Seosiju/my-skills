# my-skills CLI Project

This repository is the `my-skills` CLI project. It is not a user's skill
registry. The `my-skills.toml` and `skills/` directory here are public seed
sources and test fixtures packaged with the CLI.

Use these terms consistently:

| Term | Meaning |
| --- | --- |
| tool repo | This public CLI package repository. |
| CLI | The installed `my-skills` executable. |
| registry | A user's canonical skill repository, default `~/my-agent-skills`. |
| active root | The registry path cached under the user's XDG config directory. |
| host directory | Agent install output, such as `~/.claude/skills`. |

When a task needs to inspect or modify a user's registry, run `my-skills doctor`
first and read the `Registry:` line. If it points somewhere unexpected, ask for
or set the intended registry with `my-skills set-root <path>` before writing.

Development commands:

```bash
uv run pytest
uv build
uv run my-skills <cmd>
```

Running commands inside this source checkout should not silently repoint an
existing active registry. On a fresh machine with no active root, first-run
discovery can still cache the current directory, so check `my-skills doctor`
before doing real registry work.

Supported hosts are Claude Code, Codex, and Hermes. Do not document unsupported
hosts as supported.
