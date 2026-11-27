# Changelog

All notable changes to this project will be documented in this file.

This project follows a GitHub-first release flow. Git tags and GitHub Releases
are the release source of truth while PyPI publishing remains undecided.

## [1.5.0] - 2026-11-27

### Changed

- **15-command CLI surface (21 → 15):**
  - Unified `install`, `uninstall`, and `convert` into `sync`. `sync` now handles creates, updates, prunes, and mode conversions (`--mode link|copy`). Existing clean `link` connections automatically convert to `copy` when the effective mode is `copy`; existing clean `copy` connections convert to `link` when `--mode link` is passed or when `[targets.<host>] install_mode = "link"` is explicitly configured and the copy has no `fallback_reason` (preserving legacy `v1.4.x` copies and fallback copies under default `link` mode).
  - Updating a copy preserves its `fallback_reason`; a `copy` → `link` conversion that cannot create a link keeps the copy, records `fallback_reason`, and exits `0`. Shared-destination mode conflicts isolate only the conflicting group. `sync --check` now also reports pending `CONVERT` and `BLOCKED` entries.
  - Unified `share` into `import --from <host> [skill]` (`--plan`, `--enable`/`--disable`, `--force`, `--dry-run`, `--json`). Directory imports (`import <path>`) also support `--json` and `--disable`. `import --dry-run` stages snapshots in the OS temp directory instead of writing inside the registry.
  - Unified `status` into `skills`. Non-brief `skills --json` adds per-host `connections` (`{mode, link_kind, fallback_reason}`). Selection continues to intersect positional `SKILL...` names with `--group`, `--host`, and `--enabled`/`--disabled`.
  - Kept `diff` (including `--patch` and `--json`) for inspecting copy-mode drift.
- **YAML 1.2 frontmatter parser (`src/check/frontmatter.rs`):** Replaced the 1,444-line PyYAML 1.1 emulator with an iterative `yaml_rust2::parser::Parser` YAML 1.2 parser while keeping all measured 3-host (`claude`, `codex`, `agy`) display checks intact. YAML 1.2 scalars now format naturally in catalog JSON (for example `false` as `"false"`, `0123` as `"0123"`, `null` as `"null"`).
- **Leaner seed skill, hub check, and test layout:**
  - Compressed the bundled `my-skills` seed skill (`SKILL.md` to 55 lines + 3 reference docs: `setup.md`, `commands.md`, `hosts.md`).
  - Slimmed `scripts/hub-check.py` from 1,942 to 899 lines while preserving full live verification across Claude Code, Codex, and Antigravity CLI.
  - Consolidated 38 integration test files into 10 domain suites + `tests/common/mod.rs` with zero lost test scenarios.
- **Upgrade note:** Existing registries keep their own copy of `skills/my-skills/`. Refresh that copy after upgrading if you want agents to follow the v1.5.0 15-command workflow instead of removed commands.

### Removed

- Removed 6 redundant subcommands: `install`, `uninstall`, `convert`, `status`, `share`, and `resolve`. Invoking any of them exits `2` with a migration hint pointing to its replacement.
- Removed legacy flags `--with-governance` / `--with-status` from `skills` and `--skip-tool-install` from `bootstrap`.
- Removed the legacy `Python:` / `"python"` field from `doctor` output.

## [1.4.2] - 2026-11-26

### Fixed

- `doctor` no longer mixes two registries in one report. Run from a registry that is not the active one (for example a clone of this tool), it used to show that registry's `Hosts:` rows (`link 0 / copy 0`) next to a `Connections:` total for every record on the machine (`link 49 / copy 5`). Now `Connections:` counts only the selected registry's records, which matches the `Hosts:` rows. The rest appear on their own line, `Other registries or stale records: link x / copy y (fallback z)`. They include other registries, deleted registries, an old skills folder, and removed hosts.
- With no registry loaded (none set up, or an invalid manifest) and a readable `state.json`, `Connections:` still counts everything and says so: `... across all registries (no registry loaded)`. If `state.json` cannot be read, doctor warns and shows zeros, as before.

### Changed

- `doctor --json` `connections` gains `scope` (`"registry"` or `"machine"`) and `other` (`{link, copy, fallback}`, or `null` when the counts cover the whole machine or `state.json` could not be read). With `scope: "registry"`, `link`, `copy`, and `fallback` now count only the selected registry. Each one drops by its matching `other` count. When `other` is an object, the machine total is `link + other.link` (the same for `copy` and `fallback`). With `scope: "machine"`, the top-level counts are already the machine total. `other: null` with `scope: "registry"` means `state.json` could not be read. Run from the active registry with no other records, the values are unchanged. `fallback` is part of `copy`, as before.

## [1.4.1] - 2026-11-26

### Changed

- `init-registry` and `bootstrap` now install the managed Git hooks, so one command gets a machine ready. Pass `--no-hooks` to skip them. `bootstrap` no longer tells you to run `my-skills hooks install`.
- Every hook run adds any managed hook that is missing. After `my-skills update`, the next commit or a pull that brings changes fills a hook that a newer version added. `bootstrap` or `hooks install` adds it right away.
- Automatic install writes only missing hooks and never touches a foreign hook. It skips a registry inside a larger Git repository, a linked worktree, a set `core.hooksPath`, and a linked `.git/hooks`, and says why (`Git hooks: skipped (<reason>)`). In a hook run, a failure to add a hook is a warning and never changes Git's result.
- `hooks install` with a foreign hook present now installs the other hooks. It still leaves the foreign hook alone, prints the line to add to it, and exits `1`.
- With no registry set up, the not-found error, `doctor`, the install scripts, and `my-skills` with no command show the two ways to start: `my-skills init-registry`, or clone your registry to `"$HOME/my-agent-skills"` and run `my-skills bootstrap`. The steps are one command per line and use `$HOME`, so they also work in Windows PowerShell 5.1.
- `my-skills` with no command prints help and exits `0`, instead of a usage error.

## [1.4.0] - 2026-11-26

### Removed

- Removed the `audit` command, `--skip-audit` flags, and audit-based findings and refusals across skill workflows. Existing manifests and state can still be read. `skills --with-governance` still reports provenance.
- Removed binary signature and NUL-byte checks and encoding notices for non-UTF-8 supporting files.
- Why: each owner keeps their own agent-skills repository and decides its security. my-skills stores and delivers skills as written. For leaked keys, use a dedicated tool such as gitleaks or GitHub secret scanning.

## [1.3.2] - 2026-11-25

### Changed

- my-skills no longer refuses to connect a skill because of its `SKILL.md` content. A missing `SKILL.md` and security audit policy still block. Everything else is now a notice. That includes the 768-character description rule (it had no basis in any host), missing body-link files, name and description rules, a UTF-8 BOM, and missing or unreadable frontmatter.
- `validate` and connection commands tell you, per targeted host, how the skill will appear: not shown (and why), shown cut at N characters, or shown under a different name. The host rules were measured on Claude Code 2.1.318, Codex 0.172.0, and agy 1.5.16 and live in the host definitions. Fixing anything is optional.
- Body-link notices use a CommonMark parser (`pulldown-cmark`), so links inside code, autolinks, and URIs with a scheme are not reported.
- `my-skills new` writes the recommended format that every host shows in full: frontmatter present, `name` equal to the folder name, a description of 1,024 characters or fewer, values safely quoted, UTF-8 without a BOM. Long descriptions are kept, with a notice for hosts that would cut them.

### Added

- Regression tests with canary skills cover 16+ measured cases (missing name or description, long names and descriptions, extra fields, missing links, BOM, ` #` comments, multi-line values, CRLF, no frontmatter). They check that none of them is blocked and that each host's notice matches its measured behavior.

## [1.3.1] - 2026-11-25

### Added

- `scripts/hub-check.py` (development tool) runs Claude Code, Codex, and agy headless to confirm that every agent sees the registry's skills, and that edits, new skills, and disables reach all of them. The table it prints is the pre-release check.

### Fixed

- `validate` flags a UTF-8 BOM in `SKILL.md` as an error for Codex targets and a warning for other targets.
- `validate` warns when an inline YAML comment cuts off an unquoted `name` or `description` and shows the text agents see.

## [1.3.0] - 2026-11-25

### Added

- `my-skills new <NAME> [--group G] [--host H]... [--description TEXT] [--dry-run] [--json]`
  creates `skills/<NAME>/SKILL.md` and registers `[skills.<NAME>]` with
  `enabled = false` under `my-skills.toml.edit-lock` (without touching state).
  Checks skill/group names (`invalid_name`), `--description` (non-blank, `<= 1024`
  and within the smallest `description_max_chars` of the targeted or enabled
  built-in hosts — 768 characters, mapped to `usage`), active-root write
  policy (re-checking under `edit-lock` and using conditional active-root
  caching so a newer `set-root` is never overwritten), existing `skills/<NAME>`
  entries (`already_exists`, including files or broken links), base/local
  manifest collisions (`already_exists`), `--host` targets (`unknown_host`),
  and host-directory containment (refusing `skills/<NAME>` inside a host
  directory) before writing. Rollback removes only `SKILL.md` if `new` created
  it and the skill directory only when it holds no foreign files, reporting
  `rollback failed, remove by hand: ...` on rollback errors. Exit `0` on
  success, `2` on error.
- `post-commit` managed Git hook: `hooks install`, `hooks uninstall`,
  `hooks status`, and `doctor` now manage five hooks (`post-commit` second,
  after `pre-commit`). After checking that the repo is not a linked worktree,
  `post-commit` acquires `state.json.lock` before inspecting Git state (and
  re-checks active-root eligibility under the lock) so edits arriving while it
  waits are detected rather than copied. It inspects `HEAD` including
  conflict-resolved merge commits (`diff-tree --root -m --first-parent`), runs
  `sync --yes --quiet` (`my-skills.toml` triggers a full sync; otherwise only
  committed registered skills sync), warns if a synced skill changes during
  the copy, skips uncommitted `my-skills.toml` changes, skips dirty skill
  directories (with a warning for registered or recorded skills, and excludes
  dirty skills from both full sync and orphan pruning), skips in-progress
  `rebase`/`merge`/`cherry-pick`, and always exits `0`. **Upgrade note:** re-run
  `my-skills hooks install` on every machine (`hooks status` reports
  `post-commit` as `missing` until installed).
- `my-skills resolve <TARGET> [--json]` maps a registered skill name, canonical
  path, or configured host path (including disabled hosts, subdirectories, and
  nested files) to its canonical directory or file without hashing or updating
  the root cache or install state. Inputs that are absolute, start with `.` or
  `~`, or contain a path separator are treated as paths; all others are skill
  names. Supports linked canonical skill roots. Exit `0` covers `registered`
  and `unregistered` draft directories under `skills/`; exit `1` covers
  `foreign`, `unmanaged`, `ambiguous`, and `unsafe`; exit `2` covers unknown
  skill names (`unknown_skill` in JSON) and usage or configuration errors.
- `skills [SKILL]...` filters by positional skill names intersected (AND) with
  `--group`, `--host`, and `--enabled`/`--disabled`. `status [SKILL]...`
  accepts positional skill names and repeatable `--group` (union) while keeping
  its top-level JSON array and normal per-host labels for explicitly selected
  disabled skills (such as `MISSING`). Both reject unknown skill names with
  exit `2` (`unknown_skill`).
- `doctor` and `doctor --quick` add a `Catalog:` summary line in text output
  (per enabled host: skill count, total Unicode characters across
  descriptions, up to two longest descriptions, and invalid count when nonzero;
  or `Catalog: no enabled hosts`) and a `catalog` object (`skills`,
  `description_chars`, `longest: [{name, chars}]`, `invalid`) on each enabled
  host in `--json` (omitted when disabled or without a loaded manifest).
- Writer serialization via `state.json.lock` beside `state.json`: `install`,
  `sync`, `uninstall`, `convert`, `bootstrap`, `share`, and hook-triggered sync
  hold the lock for the whole write (`share` acquires
  `my-skills.toml.edit-lock` first, then `state.json.lock`). Contention prints
  `Waiting for another my-skills command to finish...` once after about one
  second and waits up to 30 seconds (`MY_SKILLS_STATE_LOCK_TIMEOUT_MS`
  overrides the wait for tests, capped at 30 seconds). A timeout prints
  `error: state is busy: <lock_path>`, exits `2` (`state_busy` in JSON), and
  leaves state and hosts unchanged; a busy `post-commit`, `post-merge`,
  `post-rewrite`, or `post-checkout` hook prints that diagnostic plus
  `warning: <hook> could not sync the registry` and exits `0`. Readers stay
  unlocked. Writers also re-check implicit active-root caching under a separate
  `root.lock` after acquiring `state.json.lock` so a concurrent `set-root` is
  preserved with a stderr note.

### Changed

- **Deliberate behaviour change in `install --json` and `sync --json`:**
  `install --json` and `sync --json` now apply by default and emit one JSON
  document to stdout after the final state save or rollback (removing the old
  requirement to pass `--dry-run`). Pass `--dry-run --json` for a preview, and
  drop only `--dry-run` to apply (`sync --json --check` still returns a usage
  error with exit `2`). Applied JSON sets `dry_run: false` and adds `applied`
  (boolean, `true` when every write succeeded and no action was `blocked` or
  `failed`, including no-op syncs and post-commit cleanup warnings), `summary`
  (`{created, updated, removed, unchanged, blocked, failed}`, excluding
  `skipped`), `cleanup_warnings` (`[{skill, host, path, message}]`), and
  per-action `result` (`created`, `updated`, `removed`, `unchanged`, `blocked`,
  `failed`, `skipped`), `requested_mode`, `mode` (actual mode, or `null` when
  skipped/blocked/failed before completion), `fallback_reason`, and `error`.
  Early configuration failures (`unknown_skill`, `state_busy`, `sync --json --check`)
  still emit `{"error": ...}` with exit `2`; post-plan refusals (validation,
  audit, `needs_yes`, `inactive_registry`, incompatible shared destinations, or
  late audit config failures) return the applied document with `applied: false`
  (`needs_yes`, `inactive_registry`, and post-plan config failures exit `2`;
  late copy-edit conflicts add `kind: "conflict"` and exit `1`).
- **Deliberate behaviour change in explicit `sync <SKILL>` / `--group`:**
  explicitly selecting skills by name or `--group` now validates and audits all
  wanted skills on the selected hosts, including zero-action `FRESH` links. A
  broken skill with an already-`FRESH` link now fails with exit `1` and can
  block the batch. Disabled skills whose only planned action is `REMOVE`
  (prune) bypass those checks unless `--all` puts them in scope to connect.
  Bare `sync` (including hook-triggered sync) continues to check only the
  create/update skill set, and `sync --check` stays drift-only without running
  validation or audit.
- **Deliberate behaviour change in frontmatter warnings:** `validate` and host
  checks during `install` and `sync` previews no longer warn about standard
  optional Agent Skills fields (`license`, `compatibility`, and `metadata`).
  `allowed-tools` / `allowed_tools` keep their single dedicated warning without
  a duplicate generic warning, and any other unsupported fields emit one
  combined warning per skill listing all affected hosts.
- Staged copy checks and late-edit protection: copy creation, refresh, link
  fallback, and `convert --to copy` build one private check snapshot per
  copied skill for validation, audit, and both recorded hashes, then stage from
  that snapshot onto the destination volume. `install`, `sync`, and
  `convert --to copy` previews also validate and audit planned copies from that
  filtered snapshot view. If the canonical skill is edited after the snapshot
  check, the originally checked bytes are installed and the copy is left
  `STALE`; if the installed copy is edited before an update or conversion
  replaces it, `my-skills` returns `CONFLICT` (exit `1`) and leaves the host
  edit intact.
- Batch planning detects physical destination aliases across hosts and protects
  unselected peers sharing a destination: compatible aliased copy targets share
  one physical write and record every host (and reconcile an unrecorded alias
  when a clean managed peer proves ownership), while incompatible aliases block
  the batch before writing. Single-host `install`, `sync`, `convert`, and
  `uninstall` (including `uninstall --dry-run`) block with `SHARED_DESTINATION`
  (JSON `shared_destination`, exit `1`) before writing when a planned `Remove`,
  `Update`, or `Convert` would alter or delete an entry shared by an unselected
  recorded connection. Pruning one peer while another keeps or refreshes the
  shared entry now succeeds and leaves the shared tree in place until the last
  peer is removed.
- `validate_selected` drops host rows in `validation[]` and `block_reasons`
  that only repeat skill-level errors or warnings.
- Real and `--dry-run` `convert --to copy` now run validation and the registry
  audit gate on the copy snapshot before replacing a link.
- `post-commit`, `post-merge`, `post-rewrite`, and `post-checkout` hooks are
  now no-ops inside linked Git worktrees (`git worktree add`).
- Committed updates, conversions, removals, `share`, and `import` print a
  `warning: cleanup could not remove <path>: <error>` line to stderr and still
  succeed when post-commit backup cleanup fails (distinct from rollback
  failures).

### Fixed

- `share` from a discovered working directory now records the active root only
  after adoption succeeds, leaving the root cache untouched when `share` fails
  or the state lock is busy.
- Explicit `set-root` now propagates `root.lock` and cache-write errors (exit
  `2`) instead of claiming success when the active root could not be saved.

## [1.2.0] - 2026-11-24

### Added

- Global `--root <PATH>` selects a registry for one command. It takes
  precedence over `MY_SKILLS_ROOT`, the current directory or parent manifest,
  and the saved active registry. It never saves the selection and reports
  registry source `flag`. A path without `my-skills.toml` is a usage error
  (exit `2`). Pass the path directly to `set-root` or `init-registry`, which
  reject combinations with `--root`.
- `install`, `sync`, `uninstall`, `enable`, `disable`, `audit`, and `validate`
  accept multiple skill names (`SKILL...`) and repeatable `--group` selections.
  An explicit `sync` selection limits pruning to the selected skills and
  preserves managed orphans. `enable` and `disable` apply each selection in
  one atomic manifest edit.
- `skills --json` adds `canonical_path` and `group`. Repeatable `--group`
  filters the catalog. `--brief` returns compact JSON rows and retains `error`
  when present. With `--with-governance` it nests `audit` and `provenance`
  under `governance`. In a synthetic ten-skill registry, `--brief` reduced
  stdout from 4,098 to 1,978 bytes (51.7%).
- `validate --host <HOST>` checks that host's constraints. `validate --json`
  reports `registry`, overall `ok`, and per-skill `ok`, `errors`, and `warnings`.
- Usage and configuration errors with `--json` emit an `error` object on
  stdout with `code`, `message`, and `hint`. Codes are `usage`,
  `inactive_registry`, `unknown_skill`, `unknown_group`, `unknown_host`,
  and `config`. Human diagnostics still go to stderr.
- `doctor` reports each host's `configured_mode` and owned `connections`
  counts (`link`, `copy`, `fallback`). `--quick` skips the update lookup,
  Git and hook diagnostics, full validation, and the link probe.
  `MY_SKILLS_NO_UPDATE_CHECK=1` skips only the update lookup.
- `diff <SKILL>` compares live host content with the canonical skill without
  writing. An explicit `--host` checks that target even when it is disabled
  or the manifest does not want the connection. Diff uses the install record's
  destination when available and lists file changes or shows the link target.
  `--patch` adds unified diffs for changed UTF-8 text files up to 1 MiB.
  Exit `0` means a match, `1` means a difference or missing entry, and `2`
  means an error. A `FOREIGN` entry counts as different even if its content
  matches. JSON returns `{registry, skill, different, hosts}`, with host fields
  `host`, `status`, `mode`, `destination`, `owner`, `files`
  (`{added, removed, modified}`), `patch`, `link_target`, and `link_matches`.
  Status tracks changes against install records, so identical edits on both
  sides can show `CONFLICT` with no live file differences.

### Changed

- **Deliberate behaviour change in `validate`.** It now applies the pre-commit
  gate, including host limits and the manifest audit policy. A description
  that exceeds an applicable host's 768-character limit now fails validation.
- `skills --with-governance` is the new name for `--with-status`. The old flag
  remains an alias.

## [1.1.0] - 2026-11-24

### Added

- `sync --dry-run --json` for machine-readable sync previews (`sync --json`
  without `--dry-run` exits `2`; `sync --check` takes precedence over
  `--dry-run` and cannot be combined with `--json`).
- Structured fields in `install --dry-run --json` and `sync --dry-run --json`:
  `registry` (`{path, source}`), `dry_run`, `blocked`, `applicable` (whether
  the same command without `--dry-run` would exit `0`, including `--yes` and
  active-registry checks), `block_reasons` (`validation`, `audit`, `needs_yes`,
  `inactive_registry`, or a lowercased block action such as `block_drift` or
  `block_foreign`), and `validation` (`[{skill, host, ok, errors, warnings}]`),
  while keeping the existing `actions` and `audit` keys.
- `registry` (`{path, source}`) metadata in `skills --json` and
  `share --plan|--dry-run --json`.
- Cross-registry connection ownership protection (`FOREIGN` status and
  `BLOCK_FOREIGN` action): connections whose `state.json` record points into
  another registry's `skills/` directory are never pruned, updated, converted,
  uninstalled, or overwritten. `sync` silently leaves another registry's
  connections alone when pruning, and `share --plan` recommends `skip` for
  foreign-owned host copies.
- Complete option descriptions across `--help`, plus top-level registry
  resolution, default host scope, and exit-code rules.

### Changed

- **Deliberate behaviour change — write refusal from a non-active cwd
  registry:** when the current directory's `my-skills.toml` differs from the
  saved active registry, real write commands (`install`, `sync`, `enable`,
  `disable`, `import`, `share`, `uninstall`, `convert`, `bootstrap`, and
  `hooks install`/`uninstall`) now refuse with exit code `2` instead of writing
  to the cwd registry. The error names both registries and the three ways out:
  run from the active registry, run `my-skills set-root <path>`, or set
  `MY_SKILLS_ROOT=<path>` for one command. Read-only commands still run against
  the cwd registry with a stderr note.
- **Deliberate behaviour change — `sync --dry-run` validation and audit
  parity:** `sync --dry-run` now runs the same validation and audit checks as a
  real `sync`, and exits `1` when validation fails (matching
  `install --dry-run`; otherwise `0` even when audit, plan items, `--yes`, or
  an inactive cwd registry would refuse the real run). Use `sync --check` when
  you need a non-zero exit code for any out-of-date state.
- Read-only and preview commands (`validate`, `audit`, `status`, `skills`,
  `data-path`, `share --plan`/`--dry-run`, `sync --check`, every `--dry-run`,
  and usage errors) never write the active-registry cache. Only the first real
  write from a registry — including `bootstrap` (unless `--dry-run` or
  `--skip-skill-install`) and `hooks install`/`uninstall` — records it.
- Text dry-runs (`install`, `sync`, `uninstall`, and `convert` with
  `--dry-run`) print a top-line `BLOCKED:` summary whenever items or an
  inactive cwd registry would refuse the real run, and end with
  `Re-run with --yes to apply: ...` when multi-host confirmation is required.
  With `--json`, validation warnings stay in JSON instead of being printed to
  stderr.
- JSON output (`skills --json`, `share --plan|--dry-run --json`, and dry-run
  JSON) now emits plain UTF-8 instead of `\uXXXX` escapes (roughly 45% smaller
  for non-ASCII descriptions). Audit `result_hash` fingerprints are unchanged.

### Fixed

- `sync` no longer prunes or overwrites copy or link connections installed by
  another registry on the same machine, and `sync --check` no longer treats
  `FOREIGN` records on hosts a skill does not target as drift.
- `audit.skipped` in dry-run JSON is now `true` only when an audit gate runs
  with `--skip-audit` (or `[audit] enabled = false`), not when no writes
  required auditing or validation stopped early; `sync --dry-run --skip-audit`
  now prints the skipped-audit warning in text output.
- Windows paths in JSON and CLI output (`doctor`, `skills`, `share`, `audit`,
  `install`/`sync --dry-run`, `hooks`, stderr notes, manifest errors,
  `data-path`, and `update --dry-run`) strip the `\\?\` verbatim prefix and use
  consistent `\` separators.

## [1.0.3] - 2026-11-24

### Removed

- Hermes is no longer a supported host. The built-in hosts are Claude Code,
  Codex, and Antigravity CLI. Old registries keep loading: a `[targets.hermes]`
  table with a `path` becomes an ordinary custom target, a disabled one without
  a path is ignored, and existing Hermes copies are left in place.

## [1.0.2] - 2026-11-24

### Fixed

- Skills can bundle common font and image assets whose file signatures match
  their extensions without triggering NUL-byte errors during import, validation,
  audit, or pre-commit checks.

## [1.0.1] - 2026-11-24

### Fixed

- If saving `state.json` fails while `install`, `sync`, `bootstrap`, or
  `uninstall` changes a host entry, that change is now rolled back. Rerunning
  the same command after fixing the cause succeeds instead of stopping with
  `BLOCK_CONFLICT`.
- `--dry-run` no longer writes the active-registry cache on a first run. The
  preview prints `would remember <path> as the active registry` instead, and
  still notes when another registry is active.
- `doctor` tells you to find or fix the registry first when it cannot read the
  manifest, and `bootstrap --dry-run` describes the hook step the way the real
  run does.
- Windows output: hook install paths no longer mix `/` and `\`, `set-root` no
  longer shows the `\\?\` prefix, and audit reports use `/` in file paths.

## [1.0.0] - 2026-11-24

- First stable release of the Rust CLI. It manages a canonical skill registry
  and links or copies skills into Claude Code, Codex, Hermes, and Antigravity CLI.
- Install prebuilt binaries for macOS (Apple silicon and Intel), Linux
  (x86-64 GNU), and Windows (x86-64 MSVC) with the release installers.
- Use `my-skills update --check` to check for a newer stable v1 release.
  Python v0.4.2 users can run `my-skills update` for migration instructions.

## [1.0.0-alpha.1] - 2026-11-24

### Added

- First Rust prerelease, with binaries and SHA-256 checksums for macOS
  (Apple silicon and Intel), Linux (x86-64 GNU), and Windows (x86-64 MSVC).
- Symlink and junction installation for skills, with copy fallback and
  per-host install modes.
- Shell and PowerShell installers and a Rust self-updater. Use
  `my-skills update --check --channel main` to check for prereleases.

## [0.4.2] - 2026-11-23

### Changed

- `my-skills update` no longer installs a Rust v1 tag through uv. On the
  stable channel, when a `v1.*` release exists, it stays on the newest
  compatible 0.4.x release and prints the Rust binary install steps instead.
  On the main channel, once any `v1.*` tag exists (prereleases included), it
  stops installing from `main` and prints the same steps. Existing installs
  are left untouched.
- The install links in that notice point at the release tag, so they resolve
  before and after the Rust code reaches `main`.
- `my-skills doctor` notes on its `Update:` line when a Rust v1 release is
  available.

## [0.4.1] - 2026-11-14

### Removed

- `personal-profile` from default registry seeds, the source-checkout skill
  manifest, and wheel/source distributions. New registries and source-checkout
  bootstrap installs no longer include this skill. Updating the CLI package
  preserves skills in separate personal registries, host installations, and
  machine-local data.

## [0.4.0] - 2026-10-03

### Added

- Antigravity CLI (`agy`) host support for discovery diagnostics, validation,
  install/sync, status/catalog, sharing, and uninstall through the existing
  skill management workflow.
- AGY user skills path `~/.gemini/config/skills` and workspace path
  `.agents/skills`, with copy and symlink discovery verified on CLI 1.2.16.
- Explicitly disabled AGY targets in newly initialized registries, and AGY in
  the seed skills' supported-host lists for opt-in activation.
- Regression coverage and source/installed-tool CI smoke checks for AGY.

### Changed

- Default target paths and activation now use the shared host contract. AGY
  stays disabled when omitted from a manifest; existing Claude Code, Codex,
  and Hermes activation defaults are preserved.
- Public setup and release documentation now includes the AGY opt-in flow.

## [0.3.1] - 2026-07-09

### Added

- `doctor` now reports whether the installed CLI is behind the latest stable
  release tag, with `doctor --no-update-check` for offline diagnostics.
- `update` command for explicitly updating the installed CLI to the latest
  stable release tag, plus an opt-in `--channel main` development channel.

## [0.2.0] - 2026-07-09

### Added

- `set-root` command for explicitly selecting the active registry root.
- `import --enable` for registering an imported skill as enabled immediately.
- `--version` flag, and expanded `doctor` output with the active registry root
  source, CLI version, skill counts, and state/data paths.
- GitHub Actions CI for tests, package build, and CLI release smoke checks.
- Contributor, security, changelog, and release checklist documentation.
- GitHub-first install documentation using `uv tool install
  git+https://github.com/Seosiju/my-skills.git`.

### Changed

- Running a command inside a `my-skills` directory no longer silently repoints
  the machine-wide active registry when another valid active root is cached; use
  `my-skills set-root` or `init-registry` to switch. First-run discovery still
  records the root when none is cached.
- `import` now registers copied skills in `my-skills.toml` automatically. New
  imports default to disabled unless `--enable` is passed, and re-importing an
  existing registered skill preserves its hosts and local-overlay separation.
- `import` now rejects source directories that contain symlinks, preventing the
  copy step from pulling files outside the audited source tree.
- **Breaking:** `validate` — and the always-on validation that runs before
  `install`, `sync`, `import`, and `share` — now applies the full audit
  analyzer set with a fixed internal policy. Skills that previously passed
  may now be reported:
  - Absolute user/home paths in any file are now **errors** (previously
    warnings, and previously only checked in `SKILL.md`).
  - Audit-only rules (prompt injection, dangerous command patterns,
    credential/network dataflow) now surface as `validate` **errors**.
  - MEDIUM-severity rules (e.g. network senders) now surface as warnings.
- `share --plan` no longer lists the same finding twice; validation- and
  audit-derived entries are de-duplicated by file and message.
- Runtime and system artifacts (`__pycache__`, `.git`, `.omc`, `.DS_Store`)
  are now ignored consistently by content hashing, validation scanning,
  audit, and install copies. A stray `.pyc` file in a skill directory no
  longer breaks `validate`, `audit --all`, or `install`.

### Fixed

- Running the test suite no longer overwrites the real user's my-skills state
  file and active-root cache.
- `init-registry` now creates the scaffold commit even when the target is
  already an empty git repository.
- `install`/`sync` no longer lose install state when a plan item fails
  mid-run: each successful item is recorded immediately and failures are
  reported per item with a non-zero exit.
- `skills`/`status` no longer abort when one skill has broken metadata; the
  broken row is marked `invalid` and the command still exits 0 (the JSON
  output keeps its contract for agent consumers).
- A state file written by a newer my-skills version, or with malformed
  records, is now rejected with a clear message instead of a raw traceback.
- `share` now honors the manifest audit policy when evaluating candidates
  (previously it always used the default policy).
- Package description no longer claims Gemini CLI support; supported hosts
  are Claude Code, Codex, and Hermes.

### Removed

- Internal `security.py` module; its rules now live in the audit analyzers
  (user-visible effects are listed under "Changed").

## [0.1.0] - 2026-06-30

### Added

- Initial cross-agent Agent Skill registry CLI.
- Canonical `skills/` source directory with install, sync, status, share,
  import, bootstrap, doctor, validation, and data-path commands.
- Host support for Claude Code, Codex, and Hermes.
- Copy and link install modes with drift, conflict, unmanaged, stale, missing,
  and fresh status reporting.
- Machine-local data guidance for private config and memory.
