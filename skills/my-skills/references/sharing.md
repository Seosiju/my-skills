# Share and import skills

## Share from a host

First produce a read-only plan:

```bash
my-skills share --from <claude|codex|agy> --plan --json
```

Show the user the candidate skills, validation warnings and conflicts, canonical status,
and available choices. Continue only after the user chooses to enable, disable,
or skip. Apply the selected choice:

```bash
my-skills share --from <host> <skill> --enable
my-skills share --from <host> <skill> --disable
```

Use `--force` only when the plan reports a *different* canonical skill and the
user explicitly confirms overwriting it.

## Create or import a skill

Create a new draft directly in the registry with `my-skills new`:

```bash
my-skills new <name> --group <group> --host <host>
```

`new` creates `skills/<name>/SKILL.md` and registers it disabled (`enabled = false`)
under `my-skills.toml.edit-lock`. Edit the draft, then enable and preview/commit it:

```bash
my-skills enable <name>
my-skills sync <name> --host <host> --dry-run --json   # preview
my-skills sync <name> --host <host> --json             # or commit in a Git registry
```

To bring in an existing external skill folder instead, use `import`:

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
directory before importing.

After import, edit the canonical skill. Linked installs see those edits
immediately. Use `sync` to update managed copies.

## View provenance

```bash
my-skills skills --json --with-governance
```

`--with-status` is an alias for `--with-governance`. In brief JSON rows,
provenance appears under `governance`.

Registry owners are responsible for skill security. Use dedicated tools such as gitleaks or GitHub secret scanning to check for exposed credentials.
