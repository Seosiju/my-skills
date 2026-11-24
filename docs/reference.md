# my-skills Reference

Detailed reference for commands, registry lookup, install modes, safety checks, machine-local data, and host-specific notes.

## Commands

| Command | What it does |
| --- | --- |
| `my-skills skills` | List registered skills (`--json`, `--host <HOST>`, `--enabled`, `--disabled`, `--with-status`). |
| `my-skills status` | Show connection status for every skill and host (`--json`). |
| `my-skills install [SKILL]` | Connect enabled skills to host folders (`--host <HOST>`, `--all`, `--mode link\|copy`, `--dry-run`, `--json`, `--skip-audit`, `--yes`). |
| `my-skills sync [SKILL]` | Match host folders to `my-skills.toml` (your manifest) and remove unwanted managed installs (`--host <HOST>`, `--all`, `--check`, `--no-prune`, `--dry-run`, `--json`, `--quiet`, `--skip-audit`, `--yes`). |
| `my-skills uninstall [SKILL]` | Remove managed host connections without touching the source skill (`--host <HOST>`, `--dry-run`, `--yes`). |
| `my-skills enable <SKILL>` / `disable <SKILL>` | Turn a registered skill on or off in `my-skills.toml` (`--dry-run`). |
| `my-skills import <SOURCE>` | Copy an external skill folder into `skills/` and register it as disabled (`--enable`, `--force`, `--skip-audit`, `--dry-run`). |
| `my-skills share --from <HOST> [SKILL]` | Promote a skill from a host folder into your registry (`--plan` / `--dry-run`, `--json`, `--enable`, `--disable`, `--force`, `--skip-audit`). |
| `my-skills convert --to link\|copy [SKILL]` | Switch existing managed connections between link and copy (`--host <HOST>`, `--all`, `--dry-run`, `--yes`). |
| `my-skills data-path <SKILL>` | Print a skill's machine-local data folder (`--create` to create it). |
| `my-skills update` | Update the `my-skills` binary from GitHub Releases (`--check`, `--dry-run`, `--channel stable\|main`). |
| `my-skills doctor` | Check your environment, hosts, active registry, Git hooks, and manifest (`--no-update-check`, `--json`). |

Two small details to keep in mind:

- `import` rejects source folders that contain symlinks so audits and copies cover the same file tree.
- A bare `sync` checks disabled and unregistered skills too so it can remove old installs. Pass `--no-prune` to keep them, or `--check` to report changes without writing. `sync --check` takes precedence over `--dry-run` and cannot be combined with `--json`. Both `install --json` and `sync --json` require `--dry-run`.
- JSON outputs emit plain UTF-8 (no `\uXXXX` escapes), and `skills --json`, `share --plan|--dry-run --json`, and `install|sync --dry-run --json` include `registry: {path, source}`. On Windows, displayed paths strip `\\?\` and use `\`. Audit `result_hash` fingerprints are unchanged.

## How my-skills finds your registry

Most commands look for your registry in this order:

1. The `MY_SKILLS_ROOT` environment variable, for a single-command override.
2. A `my-skills.toml` file in the current folder or a parent folder.
3. The saved active registry recorded by `init-registry`, `set-root`, or the first real write.

If you already have a saved active registry and run a command inside a different registry folder:

- **Read-only and preview commands** (`validate`, `audit`, `status`, `skills`, `data-path`, `share --plan`/`--dry-run`, `sync --check`, and every `--dry-run`) use the current folder for that run and print a note to stderr without changing your saved active registry.
- **Commands that write** (`install`, `sync`, `enable`, `disable`, `import`, `share`, `uninstall`, `convert`, `bootstrap` without `--dry-run` or `--skip-skill-install`, and `hooks install`/`uninstall`) **refuse with exit code `2`** and write nothing. The error names both registries and the three ways out: run from the active registry, switch permanently with `my-skills set-root <path>`, or set `MY_SKILLS_ROOT=<path>` for one command.
- Managed Git hooks installed by `my-skills hooks install` export `MY_SKILLS_ROOT` for their own registry; running `my-skills hooks run post-merge|post-rewrite|post-checkout` without `MY_SKILLS_ROOT` inside a non-active clone skips syncing with a warning instead of writing.

To switch your saved active registry permanently:

```bash
my-skills set-root /path/to/your/registry
my-skills doctor
```

On a machine with no saved active registry yet, only the **first real write** from a registry (`install`, `sync`, `enable`, `disable`, `import`, `share`, `uninstall`, `convert`, `bootstrap` unless `--dry-run` or `--skip-skill-install`, or `hooks install`/`uninstall`) saves that folder automatically so later commands work from anywhere. Read-only commands, `sync --check`, every `--dry-run`, and usage errors never write or change the saved active registry; previews print `would remember <path> as the active registry` instead. Run `my-skills doctor` and check the `Registry:` line whenever you want to be sure which registry a command will use.

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

- **Dry runs.** Pass `--dry-run` to preview changes without writing anything. Both `install --dry-run` and `sync --dry-run` run validation and audit just like the real command, exit `1` only on validation failure, and exit `0` otherwise — even when audit, drift, foreign ownership, missing `--yes`, or an inactive cwd registry would refuse the real run. In text mode, the first stdout line starts with `BLOCKED:` whenever anything would be refused, and a multi-host preview without `--yes` ends with `Re-run with --yes to apply: ...`. Use `sync --check` when you want a non-zero exit code for any out-of-date state.
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
| `FRESH` | The host install matches the source skill in your registry. |
| `STALE` | The source skill changed after this copy was installed; `sync` can update it. |
| `DRIFTED` | The host copy was edited locally; `sync` will not overwrite it. |
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
