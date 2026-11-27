# my-skills Reference

Detailed reference for commands, registry lookup, install modes, safety checks, machine-local data, and host-specific notes.

## Commands

| Command | What it does |
| --- | --- |
| `my-skills skills [SKILL...]` | List registered skills and per-host connection status (`--json`, `--brief`, `--host <HOST>`, `--enabled`, `--disabled`, `--group <GROUP>`). |
| `my-skills diff <SKILL>` | Compare a registered skill with host entries without writing (`--host <HOST>`, `--patch`, `--json`). |
| `my-skills sync [SKILL...]` | Reconcile host connections with `my-skills.toml` — connect, update, convert mode, and prune (`--group <GROUP>`, `--host <HOST>`, `--all`, `--mode link\|copy`, `--check`, `--no-prune`, `--dry-run`, `--json`, `--quiet`, `--yes`). |
| `my-skills enable [SKILL...]` / `disable [SKILL...]` | Turn selected skills on or off in `my-skills.toml` (`--group <GROUP>`, `--dry-run`). |
| `my-skills validate [SKILL...]` | Apply the pre-commit validation gate (`--group <GROUP>`, `--host <HOST>`, `--json`). |
| `my-skills new <NAME>` | Create a disabled draft skill under `skills/<NAME>/SKILL.md` and register it (`--group <GROUP>`, `--host <HOST>`, `--description <TEXT>`, `--dry-run`, `--json`). |
| `my-skills import <SOURCE>` | Copy an external skill folder into `skills/` and register it as disabled (`--enable`, `--disable`, `--force`, `--dry-run`, `--json`). |
| `my-skills import --from <HOST> [SKILL]` | Promote a skill from a host folder into your registry (`--plan`, `--enable`, `--disable`, `--force`, `--dry-run`, `--json`). |
| `my-skills init-registry [PATH]` | Create a private registry with seed skills, `git init`, a first commit, and the managed Git hooks (`--with-defaults`, `--no-defaults`, `--no-git`, `--no-hooks`, `--dry-run`). |
| `my-skills bootstrap` | Prepare this machine from a cloned registry: connect enabled skills and install missing Git hooks (`--host <HOST>`, `--all`, `--mode link\|copy`, `--skip-skill-install`, `--no-hooks`, `--dry-run`). |
| `my-skills set-root [PATH]` | Set the active registry root used when running outside a registry (`--dry-run`). |
| `my-skills hooks <install\|uninstall\|status>` | Install, remove, or inspect the registry's managed Git hooks (`--dry-run` on `install`/`uninstall`). |
| `my-skills data-path <SKILL>` | Print a skill's machine-local data folder (`--create` to create it). |
| `my-skills update` | Update the `my-skills` binary from GitHub Releases (`--check`, `--dry-run`, `--channel stable\|main`). |
| `my-skills doctor` | Check your environment, hosts, active registry, Git hooks, and manifest (`--quick`, `--no-update-check`, `--json`). |
| `my-skills` (no command) | Print help. With no registry set up yet, also print the two ways to start: `init-registry`, or clone and `bootstrap`. |

Invoking a command removed in v1.5.0 (`install`, `uninstall`, `convert`, `status`, `share`, `resolve`) exits `2` and prints its replacement.

### Skill selection

Commands that accept `SKILL...` take the union of the supplied registered names
and members of each repeatable `--group <GROUP>`, except `skills [SKILL...]`,
which intersects (AND) supplied `SKILL...` names with `--group`, `--host`, and
`--enabled` or `--disabled`. Each selected skill runs once. `enable` and
`disable` require a name or group and validate all names before committing one
atomic manifest edit. `skills` rejects unknown skill names with exit `2`
(`unknown_skill` in JSON).

A bare `sync` reconciles every registered skill, checks only skills with
`CREATE`, `UPDATE`, or `CONVERT` actions, and can prune managed orphan
connections. With an explicit `SKILL...` or `--group` selection, `sync` limits
pruning to unwanted connections for the selected skills and validates all wanted
skills on the selected hosts, including zero-action `FRESH` links (so a broken
skill with a `FRESH` link fails with exit `1`). Disabled skills whose only
planned action is `REMOVE` (prune) bypass those checks unless `--all` puts them
in scope to connect. `--no-prune` preserves unwanted connections. `sync --all`
also connects disabled skills within its current scope, and `sync --host <HOST>`
requires that host target to be enabled. `sync --check` reports drift (including
pending `CONVERT` and `BLOCKED` entries) without running validation; it takes
precedence over `--dry-run` and cannot be combined with `--json`. `sync --json`
applies by default (add `--dry-run` to preview, drop only `--dry-run` to apply).

`import` rejects source folders that contain symlinks to keep copied files
inside the source tree.

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

- **Read-only and preview commands** use the cwd registry for that run. Commands that load it for inspection or a preview, such as `validate`, `diff`, `import --dry-run`, and `sync --dry-run`, print a note to stderr.
- **Commands that write** (`new`, `sync`, `enable`, `disable`, `import`, `bootstrap` without `--dry-run` (with `--skip-skill-install`, only when it installs hooks), and `hooks install`/`uninstall`) refuse with exit `2` before writing (`new --dry-run` also refuses an inactive cwd registry). Run from the active registry, select a registry for one command with `--root` or `MY_SKILLS_ROOT`, or switch permanently with `my-skills set-root <PATH>`.
- Managed Git hooks (installed by `init-registry`, `bootstrap`, or `my-skills hooks install`) export `MY_SKILLS_ROOT` for their own registry; running `my-skills hooks run post-merge|post-rewrite|post-checkout` without `MY_SKILLS_ROOT` inside a non-active clone skips syncing with a warning instead of writing, and all four `post-*` hooks (`post-commit`, `post-merge`, `post-rewrite`, `post-checkout`) are no-ops in linked Git worktrees (`git worktree add`).
- **Upgrading:** since v1.4.1 a hook run adds any managed hook that is missing, so after `my-skills update` the next commit or pull fills new hooks such as `post-commit`. Run `my-skills bootstrap` (or `my-skills hooks install`) to add them right away.

To switch your saved active registry permanently:

```bash
my-skills set-root /path/to/your/registry
my-skills doctor
```

On a machine with no saved active registry yet, only the **first real write** from a registry (`new`, `sync`, `enable`, `disable`, `import`, `bootstrap` unless `--dry-run` (with `--skip-skill-install`, only when it installs hooks), or `hooks install`/`uninstall`) saves that folder automatically so later commands work from anywhere. Read-only commands, `sync --check`, every `--dry-run`, and usage errors never write or change the saved active registry; previews print `would remember <path> as the active registry` instead. Run `my-skills doctor` and check the `Registry:` line whenever you want to be sure which registry a command will use.

## JSON output

JSON is UTF-8. On Windows, displayed paths strip `\\?\` and use `\`. The
`registry` object in `skills`, `new`, `import`, `sync` (`--dry-run --json` and
applied `--json`), `validate`, and `diff` has `path` and `source`. Source labels
are `flag`, `env`, `cwd`, and `cache`.

### Errors

Usage and configuration errors with `--json` exit `2` and write one error object
to stdout. The human diagnostic goes to stderr. Validation and dry-run
checks that reach a result still return their normal report when blocked.

```json
{"error":{"code":"unknown_group","message":"unknown group: example","hint":null}}
```

Codes are `usage` (including argument parsing), `inactive_registry` (a refused
cwd write), `unknown_skill`, `unknown_group`, `unknown_host`, `already_exists`,
`invalid_name`, `state_busy` (writer lock timeout), and `config`. `hint` is a
string when guidance is available and `null` otherwise.

### Skill catalog

`skills --json` includes the absolute `canonical_path`, the manifest `group`
(or `null`), per-host `status` (uppercase state labels), and per-host
`connections` (`{mode, link_kind, fallback_reason}`) in each row. Repeatable
`--group` matches any of the supplied groups and combines with the host and
enabled-state filters. JSON represents YAML 1.2 description values as text,
including scalars such as `false` as `"false"` and `null` as `"null"`. Text
output labels skills with an empty or unavailable catalog description as
`(no description)`.

`skills --json --brief` returns the top-level `registry` and `skills` keys.
Each compact row has `name`, `enabled`, `hosts`, `group`, and `status`, plus
`error` if skill metadata could not be read, and omits `description`, `path`,
`canonical_path`, and `connections`. `--brief` applies to JSON output.

### Validation

`validate` shares its skill checks with pre-commit and full `doctor`.
A missing `SKILL.md` fails validation. Content diagnostics are warnings. `sync` delivers the existing skill
bytes and reports how the selected services display the skill, including
anything they hide, shorten, or name differently.
With no selection, `validate` checks all directories under the skills root,
including unregistered skills. It reports display warnings for enabled,
applicable hosts. Use `validate --host <HOST>` to preview how that host would
display the skill even when it is disabled or outside the skill's host list.

For consistent display, use UTF-8 without a BOM and open the file with a
YAML block between `---` lines. Set `name` to the folder name, using lowercase
alphanumeric words separated by single hyphens (up to 64 characters). Include
a description of 1,024 characters or fewer and quote values that contain YAML
syntax such as ` #`.

```json
{
  "registry": {"path": "/path/to/registry", "source": "flag"},
  "ok": true,
  "skills": [
    {
      "skill": "example",
      "ok": true,
      "errors": [],
      "warnings": ["Codex does not show this skill because its description is missing or empty. If you want it to appear, add a description to the YAML frontmatter."]
    }
  ]
}
```

`validate --json` exits `0` when all checks pass, `1` on a failed check, or `2`
on a configuration error. Doctor's text output notes when a manifest contains
legacy `[audit]` settings. Legacy audit fields in state are ignored when read
and discarded when state is written.

### Diff

`my-skills diff <SKILL>` compares the canonical skill with each enabled host
where the manifest wants a connection or an install record exists. Passing
`--host <HOST>` checks that host even if it is disabled or the manifest does
not want the connection. A destination without an install record shows
`UNMANAGED` if an entry exists and `MISSING` otherwise.

Diff uses the install record's destination when available. A directory
comparison lists files only on the host (`added`), only in the registry
(`removed`), and with changed contents (`modified`), using the drive's ignore
rules. Paths are relative to the skill root and use `/` separators. For links,
diff shows the target path and whether it matches the canonical skill without
walking the linked directory.

`--patch` shows unified diffs from canonical to host for changed UTF-8 text files
up to 1 MiB. It notes binary, oversized, and empty-file changes. `diff` exits
`0` when compared entries match, `1` for differences or missing entries, and
`2` on error. A `FOREIGN` entry counts as different even when its files or link
target match. Diff leaves the registry, host files, state, and root cache alone.

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
      "link_target": null,
      "link_matches": null
    }
  ]
}
```

`owner` is the owning registry's skills root for `FOREIGN` connections and
`null` otherwise. `patch` is a string when a requested patch has output.
For a link, `link_target` is its display path and `link_matches` records
whether it points to the canonical skill. Both fields are `null` for other
entries. For copies, status tracks both sides against their recorded install
hashes. Identical edits to a managed copy and its canonical skill can yield
`CONFLICT` with `different: false` and empty file lists.

## Importing skills (`import`)

`my-skills import <SOURCE> [--enable|--disable] [--force] [--dry-run] [--json]` copies an external skill folder (containing no symlinks) into `skills/<name>/` and registers it in `my-skills.toml` (disabled by default unless `--enable` is passed).

To adopt a skill that already lives inside a host folder:

```bash
my-skills import --from <HOST> --plan --json
my-skills import --from <HOST> <SKILL> --enable --dry-run --json
my-skills import --from <HOST> <SKILL> --enable --json
```

`--plan` (or `--dry-run` with no skill name) lists host-local candidates and recommends `import`, `skip` (already managed or foreign), or `conflict`. Importing a named skill with `--from <HOST>` requires `--enable` or `--disable`, stages and validates the copy, acquires `my-skills.toml.edit-lock` then `state.json.lock`, publishes into `skills/<SKILL>/`, and records the adopted host copy in `state.json`. `--dry-run` stages snapshots in the OS temporary directory without creating files inside the registry. Use `--force` only when intentionally overwriting a different canonical skill with the same name. Blocked imports exit `1` (`{blocked: true, skill, reason, errors, symlink}` in JSON); usage and configuration errors exit `2`.

## Creating skills (`new`)

`my-skills new <NAME> [--group G] [--host H]... [--description TEXT] [--dry-run] [--json]`
creates `skills/<NAME>/SKILL.md` and registers `[skills.<NAME>]` with
`enabled = false` in `my-skills.toml` (`group` and `hosts` are omitted unless
passed). It holds `my-skills.toml.edit-lock` for the whole operation without
taking the state lock (`--dry-run` checks lock availability and writes nothing).

`new` saves long descriptions in full and warns when a selected service
would shorten them. The selection is the explicit `--host` list, or the
enabled targets when no host is specified. JSON includes any warnings in
`warnings`.

All checks run before writing. They cover skill and group name rules
(`invalid_name`), a non-blank `--description` (`usage`), active-registry write policy
(re-checked under `edit-lock`, with conditional active-root caching so a newer
`set-root` is not overwritten), any existing `skills/<NAME>` entry (`already_exists`,
including a file or broken link), base or `my-skills.local.toml` registration
conflicts (`already_exists`), `--host` existence (`unknown_host`), and
host-directory containment (both `my-skills.toml` and `skills/<NAME>` must be
outside every target directory, checked in `--dry-run` too). If registration
fails, `new` removes `SKILL.md` only if `new` created it, removes the skill
directory only when no foreign files exist there, and reports any rollback
failure alongside the error.

```json
{
  "canonical_dir": "/path/to/registry/skills/sample-skill",
  "canonical_path": "/path/to/registry/skills/sample-skill/SKILL.md",
  "created": [
    "/path/to/registry/skills/sample-skill",
    "/path/to/registry/skills/sample-skill/SKILL.md"
  ],
  "enabled": false,
  "group": "tools",
  "hosts": ["claude"],
  "next": [
    "Edit /path/to/registry/skills/sample-skill/SKILL.md",
    "my-skills enable sample-skill",
    "my-skills sync sample-skill --dry-run --json, then commit"
  ],
  "registry": {"path": "/path/to/registry", "source": "flag"},
  "skill": "sample-skill"
}
```

With `--dry-run`, JSON also includes `"dry_run": true`. Exit `0` on success;
exit `2` on validation/name (`invalid_name`), collision (`already_exists`),
`unknown_host`, `inactive_registry`, `usage`, or `config` error.

## Git hooks

`my-skills hooks install|uninstall|status` manages five hooks in `.git/hooks`:
`pre-commit`, `post-commit`, `post-merge`, `post-rewrite`, and `post-checkout`.

- **Automatic install (v1.4.1):** `init-registry` and `bootstrap` install the
  hooks (`--no-hooks` skips that step), and every hook run adds any managed hook
  that is missing. So after `my-skills update`, the next commit or pull fills a
  hook that a newer version added. Automatic install writes only missing hooks.
  It never touches a foreign hook, and it skips a registry inside a larger
  repository, a linked worktree, a set `core.hooksPath`, and a linked
  `.git/hooks`. In those cases `init-registry` and `bootstrap` print
  `Git hooks: skipped (<reason>)`, and `hooks install` still installs there.
- **Turning hooks off:** run `my-skills hooks uninstall` (it removes all five),
  and use `bootstrap --no-hooks` later so bootstrap does not put them back. A
  hook you delete while the others stay is added back on the next hook run.
- `hooks install` rewrites the managed hooks, installs the missing ones, and
  leaves a foreign hook alone. It prints the line to add to that hook and exits
  `1` while a foreign hook remains.

- `pre-commit` validates staged changes to `my-skills.toml` and `skills/<name>/`
  from the Git index snapshot and blocks invalid commits.
- `post-commit` refreshes host connections with `sync --yes --quiet` after a
  commit passes `pre-commit`. After confirming the repo is not a linked worktree,
  it acquires `state.json.lock` before inspecting Git state (and re-checks
  active-root eligibility under the lock) so edits arriving while waiting are
  detected rather than copied. It inspects `HEAD` including conflict-resolved
  merge commits (`diff-tree --root -m --first-parent`), runs a full sync when
  `my-skills.toml` changed or syncs only committed registered skills otherwise,
  warns if a synced skill becomes dirty during the copy, skips uncommitted
  `my-skills.toml` changes, skips dirty skill folders (warning for registered
  or recorded skills and excluding them from both full sync and orphan
  pruning), skips in-progress `rebase`/`merge`/`cherry-pick`, and always exits `0`.
- `post-merge`, `post-rewrite`, and `post-checkout` are also no-ops in linked
  worktrees, and warn and exit `0` when `state.json.lock` is busy.

## Doctor

Run `my-skills doctor --quick --json` for registry metadata, manifest health,
and host and connection diagnostics. It skips the update lookup, Git and hook
diagnostics, full validation, and the link probe. In quick JSON, `update` and
`link_capability.kind` are `not_checked`. `registry_git`, `hooks`,
`hooks_error`, and `full_validation` are `null`.

Both `doctor` and `doctor --quick` report catalog description cost per enabled
host. Text output prints a `Catalog:` summary line (or `Catalog: no enabled hosts`),
and JSON attaches `catalog: {skills, description_chars, longest, invalid}` to
each enabled host row (`longest` lists up to two `{name, chars}` entries).
Counts measure Unicode characters in nonblank YAML strings across wanted
skills. Other description values and missing descriptions count as `0`
characters and are left out of `longest`. JSON retains the `invalid` key for
this count. Disabled hosts and reports without a loaded manifest omit `catalog`.

Each built-in host row includes `configured_mode` (`link`, `copy`, or `null`
when the manifest is unavailable) and `connections: {link, copy, fallback}`
for records owned by the current registry. `configured_mode` describes a new
connection. A recorded copy fallback contributes to both `copy` and `fallback`.

The `Connections:` line counts the same records as the host rows. These are
records for a supported host whose source's parent folder is the selected
registry's skills folder, after the same path normalization that the host rows
and `sync` use. So two registries that share one skills folder both count
those records. All other records in `state.json` are summed on one line,
`Other registries or stale records: link x / copy y (fallback z)`, which is
omitted when there are none. They include other registries, deleted registries,
an old skills folder, and removed hosts. With no registry loaded and a readable
`state.json`, `Connections:` counts every record and ends with
`across all registries (no registry loaded)`. If `state.json` cannot be read,
doctor warns and shows zeros.
In JSON, `connections.scope` is `registry` or `machine`. In registry scope with
a readable `state.json`, `connections.other` is `{link, copy, fallback}` (zeros
when there are none), and the machine total is `link + other.link` (the same
for `copy` and `fallback`). In machine scope, `other` is `null` and the top-level
counts are the machine total. In registry scope, `other: null` means
`state.json` could not be read.
Use `--no-update-check` or set `MY_SKILLS_NO_UPDATE_CHECK=1` to skip only the
network lookup. If diagnostics find an invalid manifest, doctor exits `1` with
the health report.

## Link or copy

New registries connect skills using links by default (symlinks on macOS and Linux, directory junctions on Windows). With a link, edits to `skills/<name>/` show up in every linked agent right away, and pruning removes only the link.

When creating a new connection, `my-skills` picks `link` or `copy` in this order:

1. `--mode link` or `--mode copy` passed to `sync` or `bootstrap`.
2. `install_mode` under `[targets.<host>]` in `my-skills.local.toml` or `my-skills.toml`.
3. `install_mode` under `[defaults]` in `my-skills.toml`.

Use `my-skills.local.toml` when one machine or agent needs copies while everything else stays on links:

```toml
[targets.agy]
install_mode = "copy"
```

`sync` handles mode conversions automatically with asymmetric safety rules:

- **Link → Copy:** when the effective mode is `copy`, a clean managed `link` is converted to `copy` automatically on `sync` (validated from a staged copy snapshot).
- **Copy → Link:** a clean managed `copy` is converted to `link` when `--mode link` is passed, or when `[targets.<host>] install_mode = "link"` is explicitly set and the copy has no `fallback_reason`. Copies installed under `[defaults] install_mode = "link"` (such as legacy v1.4.x copies or fallback copies) stay `copy` on bare `sync`. If creating the link fails during `copy` → `link` conversion, `sync` keeps the copy, records `fallback_reason`, and exits `0`.
- Updating an existing copy preserves its `fallback_reason`. Scope `--mode` with `--host <host>` when different hosts use different modes (for example Windows `agy`):

```bash
my-skills sync --host codex --mode link --dry-run
my-skills sync --host codex --mode link
```

## Safety

`my-skills` avoids silent overwrites and half-finished installs:

- **Dry runs.** Pass `--dry-run` to preview changes without writing anything. `sync --dry-run` runs validation just like the real command (including validating planned copies and `link` → `copy` conversions from the same filtered check snapshot as apply), exits `1` only on validation failure, and exits `0` otherwise — even when drift, foreign ownership, missing `--yes`, or an inactive cwd registry would refuse the real run. In text mode, the first stdout line starts with `BLOCKED:` whenever items or an inactive cwd registry would refuse the real run, and multi-host previews without `--yes` end with `Re-run with --yes to apply: ...`. Use `sync --check` when you want a non-zero exit code for any out-of-date state (including pending `CONVERT` or `BLOCKED` entries).
- **Dry-run JSON (`sync --dry-run --json`).** Emits `actions`, `registry` (`{path, source}`), `dry_run` (`true`), `blocked`, `applicable` (whether the same command without `--dry-run` would exit `0`, including `--yes` and active-registry checks), `block_reasons` (`[{skill, host, kind, message}]` with kinds `validation`, `needs_yes`, `inactive_registry`, `shared_destination`, or a lowercased block action such as `block_drift` or `block_foreign`), and `validation` (`[{skill, host, ok, errors, warnings}]`; host rows that merely repeat skill-level errors/warnings are omitted). With `--json`, validation warnings stay in JSON and do not print to stderr.
- **Applied JSON (`sync --json`, without `--dry-run`).** `sync --json` applies by default and writes one JSON document to stdout after the last state save or rollback (preview is always `--dry-run --json`; to apply, drop only `--dry-run`). The applied document keeps the preview keys with `dry_run: false` and adds `applied` (boolean, `true` when every write succeeded and no action was `blocked` or `failed`, including no-op syncs and post-commit cleanup warnings), `summary` (`{created, updated, removed, unchanged, blocked, failed}`, excluding `skipped`), `cleanup_warnings` (`[{skill, host, path, message}]`), and per-action `result` (`created`, `updated`, `removed`, `unchanged`, `blocked`, `failed`, or `skipped`), `requested_mode`, `mode` (actual mode after any fallback, or `null` when skipped/blocked/failed before completion), `fallback_reason`, and `error`. Early configuration errors (`unknown_skill`, `state_busy`, `sync --json --check`) still emit `{"error": ...}` and exit `2`; post-plan refusals (validation, missing `--yes`, `inactive_registry`, or incompatible shared destinations) return the applied document with `applied: false` (missing `--yes` sets `blocked: false`, `applicable: false`, `kind: "needs_yes"`, and exits `2`; late host copy-edit conflicts add `kind: "conflict"` to `block_reasons` and exit `1`).
- **Confirmation for multi-host writes.** Writing to more than one host asks for confirmation: review a dry run, then pass `--yes`. Connecting only the built-in starter skills to default host paths is exempt.
- **Existing folders are never overwritten.** If a host folder already contains an unmanaged directory at the destination path, `my-skills` stops instead of replacing it.
- **Connections owned by another registry are protected.** A managed connection belongs to the registry whose `skills/` directory contains its recorded `source` path. `sync` silently leaves another registry's connections alone when pruning, `import --from <host> --plan` recommends `skip` for them, and `sync` and `import --from` refuse to modify or overwrite them.
- **Locally edited copies are protected.** If you edit a copied skill inside a host folder, `sync` flags it and leaves your edits untouched.
- **Failed installs roll back.** Each write stages in a temporary folder and swaps into place. If the swap or saving the install state fails, `my-skills` rolls back so you can fix the cause and re-run cleanly. When a committed update, conversion, removal, or `import` succeeds but deleting the temporary backup fails, `my-skills` prints a `warning: cleanup could not remove ...` diagnostic to stderr and keeps the committed result.
- **Staged copy snapshots, late-edit protection, and shared destinations.** Copy installs, refreshes, link fallbacks, and `link` → `copy` conversions validate and stage from one private check snapshot per skill onto the destination volume. If the canonical skill is edited after the snapshot is taken, the originally checked bytes are installed and the copy is left `STALE`. Updates and conversions away from copy mode also re-hash the installed host directory immediately before moving it aside; if a late host edit changed the digest, that item returns `CONFLICT` (exit `1`), preserves the host edit, and leaves its record untouched. Compatible shared-destination copy aliases across hosts apply once and record all hosts, while incompatible mode aliases isolate and block only that conflicting group. Single-host `sync` blocks with `SHARED_DESTINATION` (`shared_destination`, exit `1`) before writing if a planned `Remove`, `Update`, or `Convert` would alter or delete an entry shared by an unselected recorded peer; pruning one peer while another keeps or refreshes the shared entry succeeds and leaves the shared tree intact until the last peer is removed.
- **Writer locking (`state.json.lock` and `root.lock`).** Writing commands (`sync`, `bootstrap`, `import --from`, and hook-triggered sync) serialize on `state.json.lock` beside `state.json` (`import --from` acquires `my-skills.toml.edit-lock` first, then `state.json.lock`). After about one second of contention, stderr prints `Waiting for another my-skills command to finish...` once. The wait is capped at 30 seconds; on timeout, writers fail with `error: state is busy: <path>` and exit `2` (`state_busy` in JSON), while `post-*` Git hooks warn on stderr and exit `0` so Git finishes. Implicit active-root writes re-check eligibility under `root.lock` after acquiring `state.json.lock` so a newer `set-root` selection is preserved with a stderr note; explicit `set-root` propagates `root.lock` and write errors (exit `2`). The `MY_SKILLS_STATE_LOCK_TIMEOUT_MS` environment variable is test-only and overrides the wait up to 30 seconds. Readers stay unlocked.

Install state is stored locally per machine (under `$XDG_STATE_HOME` or `~/.local/state/my-skills/`) and is never committed to Git.

`skills` and `sync` assign every `(skill, host)` pair one of these states:

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

When `sync` or `import --from` acts on a skill, `DRIFTED`, `CONFLICT`, `UNMANAGED`, `BROKEN`, and `FOREIGN` (planned as `BLOCK_FOREIGN`) block that connection and make the real command exit non-zero. During `sync` pruning, connections owned by another registry are silently left alone.

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
- `my-skills` — Guides your agents through listing, creating, importing, and syncing skills with the CLI.
- `my-jira` — Disabled-by-default Jira/Atlassian starter skill showing the machine-local `config.example.json` pattern.
