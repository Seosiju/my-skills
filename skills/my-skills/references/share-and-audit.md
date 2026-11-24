# Share, Import, And Security Audit

## Share from a host

First produce a read-only plan:

```bash
my-skills share --from <claude|codex|agy> --plan --json
```

Show the user the candidate skills, validation/audit risks, canonical status,
and available choices. Continue only after the user chooses to enable, disable,
or skip. Apply the selected choice:

```bash
my-skills share --from <host> <skill> --enable
my-skills share --from <host> <skill> --disable
```

Use `--force` only when the plan reports a *different* canonical skill and the
user explicitly confirms overwriting it.

Never use `--skip-audit` for share/import unless the user explicitly accepts the
audit risk. `--force` only answers "may overwrite?"; it does not bypass audit.

## Create or import a skill

For a new skill, write it in a temporary directory first, then import it into
the registry:

```bash
my-skills import /path/to/skill
my-skills enable <skill>
my-skills install <skill> --host <host>
```

`import` copies the directory into canonical `skills/` and registers it in
`my-skills.toml`. New imports are disabled by default. Use `--enable` when the
user explicitly wants to register the skill as enabled immediately:

```bash
my-skills import /path/to/skill --enable
my-skills install <skill> --host <host>
```

Import sources must not contain symlinks. Copy real files into the source
directory before importing so audit and copy inspect the same tree.

After import, edit the canonical skill. Linked installs see those edits
immediately. Use `sync` to update managed copies.

## Audit

Run audit before a write when the user asks about risk, provenance, or whether a
skill is safe to install:

```bash
my-skills audit <skill> --json
my-skills audit --all --json
my-skills skills --json --with-status
```

Use `audit --all --json` before applying multiple skills so bundle-level and
cross-skill findings are visible.

If audit returns `blocked: true`, stop and show the finding. Do not install,
sync, share, or import that skill. The only bypass is `--skip-audit`, and that
requires explicit user approval for the exact command.
