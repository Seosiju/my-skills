# my-skills Reference

Detailed reference for commands, registry lookup, install modes, safety checks, machine-local data, and host-specific notes.

## Commands

| Command | What it does |
| --- | --- |
| `my-skills skills` | List registered skills (`--json`, `--brief`, `--host <HOST>`, `--enabled`, `--disabled`, `--group <GROUP>`, `--with-governance`). |
| `my-skills status` | Show connection status for every skill and host (`--json`). |
| `my-skills diff <SKILL>` | Compare a registered skill with host entries without writing (`--host <HOST>`, `--patch`, `--json`). |
| `my-skills install [SKILL...]` | Connect skills to host folders (`--group <GROUP>`, `--host <HOST>`, `--all`, `--mode link\|copy`, `--dry-run`, `--json`, `--skip-audit`, `--yes`). |
| `my-skills sync [SKILL...]` | Reconcile host connections with `my-skills.toml` (`--group <GROUP>`, `--host <HOST>`, `--all`, `--check`, `--no-prune`, `--dry-run`, `--json`, `--quiet`, `--skip-audit`, `--yes`). |
| `my-skills uninstall [SKILL...]` | Remove managed host connections (`--group <GROUP>`, `--host <HOST>`, `--dry-run`, `--yes`). |
| `my-skills enable [SKILL...]` / `disable [SKILL...]` | Turn selected skills on or off in `my-skills.toml` (`--group <GROUP>`, `--dry-run`). |
| `my-skills validate [SKILL...]` | Apply the pre-commit validation gate (`--group <GROUP>`, `--host <HOST>`, `--json`). |
| `my-skills audit [SKILL...]` | Audit canonical skills for security risks (`--group <GROUP>`, `--all`, `--json`). |
| `my-skills import <SOURCE>` | Copy an external skill folder into `skills/` and register it as disabled (`--enable`, `--force`, `--skip-audit`, `--dry-run`). |
| `my-skills share --from <HOST> [SKILL]` | Promote a skill from a host folder into your registry (`--plan` / `--dry-run`, `--json`, `--enable`, `--disable`, `--force`, `--skip-audit`). |
| `my-skills convert --to link\|copy [SKILL]` | Switch existing managed connections between link and copy (`--host <HOST>`, `--all`, `--dry-run`, `--yes`). |
| `my-skills data-path <SKILL>` | Print a skill's machine-local data folder (`--create` to create it). |
| `my-skills update` | Update the `my-skills` binary from GitHub Releases (`--check`, `--dry-run`, `--channel stable\|main`). |
| `my-skills doctor` | Check your environment, hosts, active registry, Git hooks, and manifest (`--quick`, `--no-update-check`, `--json`). |

### Skill selection

Commands that accept `SKILL...` take the union of the supplied registered names
and members of each repeatable `--group <GROUP>`. Each selected skill runs once.
`uninstall`, `enable`, and `disable` require a name or group. A single name on
`uninstall` can also target a managed connection for a skill you removed from
the manifest. `enable` and `disable` validate all names before committing one
atomic manifest edit.

A bare `sync` reconciles every registered skill and can prune managed orphan
connections. With a selection, `sync` can prune only unwanted connections for
the selected skills. `--no-prune` preserves unwanted connections. `sync --check`
takes precedence over `--dry-run` and cannot be combined with `--json`. Both
`install --json` and `sync --json` require `--dry-run`.

`install` defaults to enabled skills and accepts explicitly selected disabled
skills. `install --all` includes disabled skills in the default selection.
`sync --all` also connects disabled skills within its current scope.
`import` rejects source folders that contain symlinks so audits and copies
cover the same file tree.

## How my-skills finds your registry

Most commands look for your registry in this order:

1. Global `--root <PATH>` for the current invocation.
2. The `MY_SKILLS_ROOT` environment variable.
3. A `my-skills.toml` file in the current folder or a parent folder.
4. The saved active registry recorded by `init-registry`, `set-root`, or the first real write.

`--root` never saves the active root. Registry metadata reports `source: "flag"`.
A path without `my-skills.toml` gives a usage error and exit `2`. `set-root` and
`init-registry` take the path as a positional argument and reject `--root`.

If lookup picks a cwd registry other than the saved active registry:

- **Read-only and preview commands** use the cwd registry for that run. Commands that load it for inspection or a preview, such as `validate`, `diff`, and `sync --dry-run`, print a note to stderr.
- **Commands that write** (`install`, `sync`, `enable`, `disable`, `import`, `share`, `uninstall`, `convert`, `bootstrap` without `--dry-run` or `--skip-skill-install`, and `hooks install`/`uninstall`) refuse with exit `2` before writing. Run from the active registry, select a registry for one command with `--root` or `MY_SKILLS_ROOT`, or switch permanently with `my-skills set-root <PATH>`.
- Managed Git hooks installed by `my-skills hooks install` export `MY_SKILLS_ROOT` for their own registry; running `my-skills hooks run post-merge|post-rewrite|post-checkout` without `MY_SKILLS_ROOT` inside a non-active clone skips syncing with a warning instead of writing.

To switch your saved active registry permanently:

```bash
my-skills set-root /path/to/your/registry
my-skills doctor
```

On a machine with no saved active registry yet, only the **first real write** from a registry (`install`, `sync`, `enable`, `disable`, `import`, `share`, `uninstall`, `convert`, `bootstrap` unless `--dry-run` or `--skip-skill-install`, or `hooks install`/`uninstall`) saves that folder automatically so later commands work from anywhere. Read-only commands, `sync --check`, every `--dry-run`, and usage errors never write or change the saved active registry; previews print `would remember <path> as the active registry` instead. Run `my-skills doctor` and check the `Registry:` line whenever you want to be sure which registry a command will use.

## JSON output

JSON is UTF-8. On Windows, displayed paths strip `\\?\` and use `\`. The
`registry` object in `skills`, `share` previews, `install` and `sync` previews,
`validate`, and `diff` has `path` and `source`. Source labels are `flag`,
`env`, `cwd`, and `cache`.

### Errors

Usage and configuration errors with `--json` exit `2` and write one error object
to stdout. The human diagnostic goes to stderr. Validation, audit, and dry-run
checks that reach a result still return their normal report when blocked.

```json
{"error":{"code":"unknown_group","message":"unknown group: example","hint":null}}
```

Codes are `usage` (including argument parsing), `inactive_registry` (a refused
cwd write), `unknown_skill`, `unknown_group`, `unknown_host`, and `config`.
`hint` is a string when guidance is available and `null` otherwise.

### Skill catalog

`skills --json` includes the absolute `canonical_path` and the manifest `group`
(or `null`) in each row. Repeatable `--group` matches any of the supplied groups
and combines with the host and enabled-state filters.

`skills --json --brief` returns the same top-level `registry` and `skills`
keys. Each brief row has only `name`, `enabled`, `hosts`, `group`, and `status`.
`--with-governance` adds `governance: {audit, provenance}` to a brief row. Full
rows put `audit` and `provenance` at the row level. `--with-status` is an alias
for `--with-governance`. `--brief` applies to JSON output.

### Validation and audit

`validate` runs the same gate as pre-commit and doctor's full validation. This
includes standard validation, the fixed security floor, applicable enabled
host limits, and manifest audit policy. With no selection, it checks all
directories under the skills root, including unregistered skills. Use
`--host <HOST>` to check only the named host's constraints. An over-limit
description fails validation.

```json
{
  "registry": {"path": "/path/to/registry", "source": "flag"},
  "ok": false,
  "skills": [
    {
      "skill": "example",
      "ok": false,
      "errors": ["codex description exceeds 768 characters"],
      "warnings": []
    }
  ]
}
```

`validate --json` exits `0` when all checks pass, `1` on a failed check, or `2`
on a configuration error. `audit --json` returns a single result for one
explicitly selected skill. For a default or multi-skill selection it returns
`blocked`, `bundle_findings`, and `skills`.

### Diff

`my-skills diff <SKILL>` compares the canonical skill with each enabled host
where the manifest wants a connection or an install record exists. Use
`--host <HOST>` to narrow that set. A directory comparison lists files only on
the host (`added`), only in the registry (`removed`), and with changed contents
(`modified`), using the drive's ignore rules. Paths are relative to the skill
root and use `/` separators. Link entries report whether the link points to the
canonical skill without walking the linked directory.

`--patch` shows unified diffs from canonical to host for changed UTF-8 text files
up to 1 MiB. It notes binary, oversized, and empty-file changes. `diff` exits
`0` when compared entries match, `1` for differences or missing entries, and
`2` on error. It leaves the registry, host files, state, and root cache alone.

```json
{
  "registry": {"path": "/path/to/registry", "source": "flag"},
  "skill": "example",
  "different": true,
  "hosts": [
    {
      "host": "codex",
      "status": "DRIFTED",
      "mode": "copy",
      "destination": "/path/to/host/example",
      "owner": null,
      "files": {"added": [], "removed": [], "modified": ["SKILL.md"]},
      "patch": null,
      "link_matches": null
    }
  ]
}
```

`owner` is the owning registry's skills root for `FOREIGN` connections and
`null` otherwise. `patch` is a string when a requested patch has output.
`link_matches` is a boolean for link entries and `null` for other entries.
For copies, status tracks both sides against their recorded install hashes. If identical
edits were made to both sides, a copy can show `CONFLICT` while `diff` reports
`different: false` and empty file lists.

## Doctor

Run `my-skills doctor --quick --json` for registry metadata, manifest health,
and host and connection diagnostics. It skips the update lookup, Git and hook
diagnostics, full validation, and the link probe. In quick JSON, `update` and
`link_capability.kind` are `not_checked`. `registry_git`, `hooks`,
`hooks_error`, and `full_validation` are `null`.

Each built-in host row includes `configured_mode` (`link`, `copy`, or `null`
when the manifest is unavailable) and `connections: {link, copy, fallback}`
for records owned by the current registry. `configured_mode` describes a new
connection. A recorded copy fallback contributes to both `copy` and `fallback`.
Use `--no-update-check` or set `MY_SKILLS_NO_UPDATE_CHECK=1` to skip only the
network lookup. If diagnostics find an invalid manifest, doctor exits `1` with
the health report.

## Link or copy

New registries connect skills using links by default (symlinks on macOS and Linux, directory junctions on Windows). With a link, edits to `skills/<name>/` show up in every linked agent right away, and uninstalling removes only the link.

When creating a new connection, `my-skills` picks `link` or `copy` in this order:

1. `--mode link` or `--mode copy` passed to `install`.
2. `install_mode` under `[targets.<host>]` in `my-skills.local.toml` or `my-skills.toml`.
3. `install_mode` under `[defaults]` in `my-skills.toml`.

Use `my-skills.local.toml` when one machine or agent needs copies while everything else stays on links:

```toml
[targets.agy]
install_mode = "copy"
```

Existing connections keep the mode they were installed with. To switch existing connections, preview first and then convert:

```bash
my-skills convert --to link --dry-run
my-skills convert --to link --yes
```

With `--host all`, `convert` skips any host that has an explicit `[targets.<host>] install_mode` different from `--to`. Passing an explicit `--host <name>` converts that host anyway.

## Safety

`my-skills` avoids silent overwrites and half-finished installs:

- **Dry runs.** Pass `--dry-run` to preview changes without writing anything. Both `install --dry-run` and `sync --dry-run` run validation and audit just like the real command, exit `1` only on validation failure, and exit `0` otherwise — even when audit, drift, foreign ownership, missing `--yes`, or an inactive cwd registry would refuse the real run. In text mode, the first stdout line starts with `BLOCKED:` whenever items (`install`/`sync`) or an inactive cwd registry (`install`/`sync`/`uninstall`/`convert`) would refuse the real run, and multi-host previews without `--yes` end with `Re-run with --yes to apply: ...`. Use `sync --check` when you want a non-zero exit code for any out-of-date state.
- **Dry-run JSON (`install|sync --dry-run --json`).** Emits `actions`, `audit`, `registry` (`{path, source}`), `dry_run`, `blocked`, `applicable` (whether the same command without `--dry-run` would exit `0`, including `--yes` and active-registry checks), `block_reasons` (`[{skill, host, kind, message}]` with kinds `validation`, `audit`, `needs_yes`, `inactive_registry`, or a lowercased block action such as `block_drift` or `block_foreign`), and `validation` (`[{skill, host, ok, errors, warnings}]`). With `--json`, validation warnings stay in JSON and do not print to stderr.
- **Confirmation for multi-host writes.** Writing to more than one host asks for confirmation: review a dry run, then pass `--yes`. Installing only the built-in starter skills to default host paths is exempt.
- **Existing folders are never overwritten.** If a host folder already contains an unmanaged directory at the destination path, `my-skills` stops instead of replacing it.
- **Connections owned by another registry are protected.** A managed connection belongs to the registry whose `skills/` directory contains its recorded `source` path. `sync` silently leaves another registry's connections alone when pruning, `share --plan` recommends `skip` for them, and `install`, `sync`, `convert`, `uninstall`, and `share` refuse to modify or overwrite them.
- **Locally edited copies are protected.** If you edit a copied skill inside a host folder, `sync` flags it and leaves your edits untouched.
- **Failed installs roll back.** Each write stages in a temporary folder and swaps into place. If the swap or saving the install state fails, `my-skills` rolls back so you can fix the cause and re-run cleanly.

Install state is stored locally per machine (under `$XDG_STATE_HOME` or `~/.local/state/my-skills/`) and is never committed to Git.

`status` and `sync` assign every `(skill, host)` pair one of these states:

| State | Meaning |
| --- | --- |
| `FRESH` | The managed link points to the canonical skill, or both copy hashes match the install record. |
| `STALE` | The source skill changed after this copy was installed; `sync` can update it. |
| `DRIFTED` | The host entry changed locally. `sync` protects the local change. |
| `CONFLICT` | Both the source skill and the host copy changed; resolve it manually. |
| `MISSING` | The skill is registered for this host, but not installed yet. |
| `UNMANAGED` | A folder already exists at the host path with no managed install record. |
| `BROKEN` | A managed link points to a source skill folder that no longer exists. |
| `FOREIGN` | A managed connection at this host path belongs to another registry and is left untouched. |
| `UNSUPPORTED` | The skill's `hosts` list excludes this host. |

When `install`, `sync`, `convert`, `uninstall`, or `share` acts on a skill, `DRIFTED`, `CONFLICT`, `UNMANAGED`, `BROKEN`, and `FOREIGN` (planned as `BLOCK_FOREIGN`) block that connection and make the real command exit non-zero. During `sync` pruning, connections owned by another registry are silently left alone.

## Machine-local data and secrets

Keep accounts, tokens, machine paths, and local state out of your registry and out of Git.

Skills that store local files use a shared, machine-local data directory resolved by `data-path`:

```bash
my-skills data-path cli-inventory
my-skills data-path cli-inventory --create
```

For skills that need private settings, commit only a template such as `skills/<name>/config.example.json`, then copy it to the skill's data directory and edit the real `config.json` there:

```bash
config_dir="$(my-skills data-path my-jira --create)"
cp skills/my-jira/config.example.json "$config_dir/config.json"
$EDITOR "$config_dir/config.json"
```

Put machine-specific manifest overrides in `my-skills.local.toml` and any scratch or machine-only files in `local/`. Both are listed in `.gitignore` for new registries.

## Antigravity CLI (agy)

Antigravity CLI (`agy`) is opt-in and disabled by default in new registries. To enable it, set `[targets.agy]` in `my-skills.toml` and include `"agy"` in each skill's `hosts` list:

```toml
[targets.agy]
enabled = true
scope = "user"
path = "~/.gemini/config/skills"
```

The project-scope path is `.agents/skills`.

**Windows note:** `my-skills` creates directory junctions for links on Windows, and Antigravity CLI does not discover skills through junctions. Set `install_mode = "copy"` under `[targets.agy]` in `my-skills.local.toml` so `agy` uses copies while other hosts keep links. When verifying discovery on Windows, run `agy -p '/skills'` from PowerShell rather than Git Bash, because Git Bash rewrites `/skills` into a Windows path.

**Versions checked:** Global-path discovery was verified on macOS with Antigravity CLI **1.2.16** and **1.5.15** (both folders and symlinks worked), and on Windows with **1.5.14** (folders worked; junctions were not discovered).

## Included starter skills

- `cli-inventory` — Scans the CLI tools installed on this machine and saves a machine-local inventory you can read back quickly.
- `my-skills` — Guides your agents through listing, importing, sharing, installing, and syncing skills with the CLI.
- `my-jira` — Disabled-by-default Jira/Atlassian starter skill showing the machine-local `config.example.json` pattern.
