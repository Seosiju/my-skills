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

If a command fails with `my-skills.toml not found`, the CLI could not locate a
registry. If you already have one, use its folder for a single command:

```bash
my-skills --root /path/to/registry doctor --quick
```

`--root` takes precedence over `MY_SKILLS_ROOT`, the current directory or parent
manifest, and the saved active root. The command never saves the override, and
doctor shows source `flag`. A folder without `my-skills.toml` gives exit `2`.

Create a registry if you need one:

```bash
my-skills init-registry
```

Pass the registry path as the argument to `init-registry` to create it, or to
`set-root` to save it as active. Both commands reject `--root`.

## New machine bootstrap and Git hooks

On a new machine, run `bootstrap` from a clone of the registry. It reconciles
host connections and installs the Git hooks (`pre-commit`, `post-commit`,
`post-merge`, `post-rewrite`, and `post-checkout`); `--no-hooks` skips them.
`init-registry` installs them too. With no registry set up, `my-skills` with no
command prints both ways to start.

- Every hook run adds any managed hook that is missing, so after
  `my-skills update` the next commit or pull fills a hook a newer version added.
  `bootstrap` or `hooks install` adds them right away.
- Automatic install never touches a foreign hook and skips a registry inside a
  larger repository, a linked worktree, a set `core.hooksPath`, and a linked
  `.git/hooks` (`Git hooks: skipped (<reason>)`). `hooks install` still installs
  there.
- To turn hooks off, run `hooks uninstall`, and pass `--no-hooks` to later
  `bootstrap` runs.
- `post-commit` refreshes host connections after a commit passes `pre-commit`
  (skipping uncommitted `my-skills.toml` or dirty skills with a warning).
- All four `post-*` hooks are no-ops in linked Git worktrees (`git worktree add`).
