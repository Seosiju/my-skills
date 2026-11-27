# Registry Setup and Git Hooks

Follow the [CLI setup instructions](https://github.com/Seosiju/my-skills/tree/main#install-the-cli)
for your platform, then run `my-skills --version`.

Run `my-skills init-registry [path]` outside the tool repo. The default is
`~/my-agent-skills`. The CLI seeds default skills, initializes Git, and saves
the active root. Use `--no-defaults` for an empty registry, `--no-git` for a
plain folder, `--no-hooks` to skip hooks, or `--dry-run` to preview.

Preview host delivery with `my-skills sync --dry-run --json`. Apply the reviewed
plan with `my-skills sync --json`. Multi-host writes require explicit approval
and `--yes`.

## Registry selection

If you see `my-skills.toml not found`, run `my-skills init-registry [path]` to create a registry or `my-skills set-root <path>` to select an existing one.

For an existing registry, run `my-skills --root <path> doctor --quick`.
The one-command override takes precedence over `MY_SKILLS_ROOT`, cwd
discovery, and the cached active root. Use `my-skills set-root <path>` to
save the active root. Both `set-root` and `init-registry` take a path argument
and reject `--root`. If cwd discovery finds a different registry than the
active one, write commands require an explicit selection or the active cwd.

## New machine and hook repair

Clone the registry and run `my-skills bootstrap --dry-run`, then
`my-skills bootstrap` for setup and host reconciliation. Use
`--skip-skill-install` to set up hooks without reconciling skills.

Git setup in `init-registry` and `bootstrap` adds five managed hooks:
`pre-commit`, `post-commit`, `post-merge`, `post-rewrite`, and `post-checkout`.
A managed hook adds any missing managed hooks on its next run.
Use `my-skills hooks status` to inspect and `my-skills hooks install` to repair.

Automatic hook setup preserves foreign hooks and skips nested registries,
linked worktrees, custom `core.hooksPath`, and linked `.git/hooks`. The four
post hooks skip reconciliation in linked worktrees. `pre-commit` checks staged
skills. Post hooks reconcile connections and skip dirty skills or a dirty
manifest. Use `--no-hooks` on future setup runs if you remove managed hooks.

Preview a CLI upgrade with `my-skills update --check` or `my-skills update --dry-run`.
