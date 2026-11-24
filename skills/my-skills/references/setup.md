# Setup, Recovery, And New Machines

## First-time setup (the front door)

If the Rust CLI is missing, follow the release installer instructions in the
[tool repo README](https://github.com/Seosiju/my-skills/tree/main#install-the-cli).
Run `my-skills --version` to check the installation.

Create the registry from outside the tool repo. `init-registry` prompts for a
location (default `~/my-agent-skills`), seeds the public-safe default skills,
and runs `git init`:

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

As a temporary override, `export MY_SKILLS_ROOT=/path/to/registry` if one
already exists elsewhere (for example on another machine you cloned, or when
running a one-off command against a second registry without switching the active
root).

## New machine bootstrap and Git hooks

On a new machine, run `bootstrap` from a clone of the registry to reconcile its
host connections. Run `my-skills hooks install` separately for Git hooks.
