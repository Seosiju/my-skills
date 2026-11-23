# my-skills

**Write an Agent Skill once. Use it in every agent.**

_One canonical home for your skills — installed, synced, and shared across Claude Code, Codex, Hermes, and Antigravity CLI._

**English** | [한국어](README.ko.md)

[![Rust](https://img.shields.io/badge/built%20with-Rust-orange.svg)](https://www.rust-lang.org/)
[![spec: Agent Skills](https://img.shields.io/badge/spec-Agent%20Skills-green.svg)](https://agentskills.io/specification)
[![License: MIT](https://img.shields.io/badge/license-MIT-yellow.svg)](LICENSE)

`my-skills` is a small CLI that keeps your [Agent Skills](https://agentskills.io/specification)
in a single place and installs the same skill into every AI coding agent you use.
Edits to a linked skill are visible in each host. For copy installs, `sync`
propagates canonical edits and flags locally edited copies.

## Why

- **Author once, run anywhere** — the same skill works in Claude Code, Codex, Hermes, and Antigravity CLI.
- **Live links by default** — new registries use links, and copy mode detects local edits before replacing a copy.
- **Machine-local stays local** — secrets, paths, and accounts never land in a canonical skill or in git.
- **CI-friendly** — `validate`, `install`, and `sync --check` exit non-zero on errors or drift.

## Antigravity CLI (agy)

AGY is supported **opt-in**. Existing manifests that omit it keep their existing
Claude Code/Codex/Hermes default targets. New registries explicitly disable AGY,
while seeded skills include it in their supported hosts. To enable it, add:

```toml
[targets.agy]
enabled = true
scope = "user"
path = "~/.gemini/config/skills"
```

Include `agy` in each desired skill's `hosts` list. `--host all` selects only
enabled targets; explicit `--host agy` can select a disabled target, just like
other explicit hosts. Use normal validation, audit, and dry-run before migration:

```bash
my-skills doctor --no-update-check
my-skills install <skill> --host agy --dry-run --json
my-skills install <skill> --host agy
my-skills skills --host agy --json
agy -p '/skills' --output-format json
```

The project path is `.agents/skills`. Global-path discovery for both regular
folders and symlinks was verified with **Antigravity CLI 1.2.16**; `/skills`
used zero model turns and tokens in that verification. This does not validate
Antigravity IDE or every CLI version. Official documentation has also described
`~/.gemini/antigravity-cli/skills`, so recheck discovery on other versions and use
a target path override if needed. Preserve existing unmanaged skills: compare,
back up outside discovery paths, and migrate through the usual collision checks.
Public Hermes support remains unchanged; personal registries may disable it.

## How it works

### One tool, two repos

The tool repo (this repository) is what you install. Your registry (default
`~/my-agent-skills`, created by `init-registry`) is where your skills live.
The source checkout provides the CLI and its public seed files. Keep your registry in a separate directory.

A skill is just a directory under your registry's `skills/<name>/` with a
`SKILL.md` that carries YAML frontmatter (`name`, `description`) — the
[Agent Skills](https://agentskills.io/specification) standard. Your registry's
`skills/` directory is the **canonical** source of truth.

Each agent (Claude Code, Codex, Hermes, Antigravity CLI) is a **host**.
`install` connects canonical skills to hosts using links or copies. `sync`
reconciles configured connections and removes managed installs that are no
longer wanted by default. It protects locally edited copies from overwrites.

Think of your registry's `skills/` as your source and each host directory as a build
output:

```mermaid
flowchart LR
    edit["✏️ You"] --> src["registry skills/&lt;name&gt;<br/><b>source (canonical)</b>"]
    ext["external skill"] -.->|import / share| src
    src -->|"install (link or copy)"| out
    src -->|"sync (reconcile)"| out

    subgraph out["build output (each host)"]
        c["Claude Code"]
        x["Codex"]
        h["Hermes"]
        a["Antigravity CLI (agy)"]
    end

    out -. "if a copy is edited directly" .-> drift["⚠️ DRIFTED<br/>sync won't overwrite"]
```

## Install the CLI

The `dev` branch contains Rust v1 in development. A Rust v1 release has yet to
be published. Build from source with Git and a stable Rust toolchain. On
Windows, use the MSVC toolchain and build tools.

```bash
git clone --branch dev https://github.com/Seosiju/my-skills.git
cd my-skills
cargo install --locked --path .
my-skills --version
```

Ensure Cargo's bin directory is on PATH. If another `my-skills` is already
installed, check that PATH selects the Rust binary. To build without installing,
run `cargo build --locked --release` from the tool repo. The executable is
`target/release/my-skills` (`target\release\my-skills.exe` on Windows).

Once Rust v1 binaries are published, run the installer from a Rust tool repo
checkout. On macOS (Apple silicon or Intel) and Linux (x86-64 GNU), run
`sh scripts/install.sh`. On Windows (x86-64), run
`powershell -File scripts/install.ps1`. Each installer verifies the archive
against the release's `SHA256SUMS`.

The scripts default to the latest GitHub release URL. Set `MY_SKILLS_VERSION`
to the exact published Rust tag until a stable Rust release is the latest
release, or whenever you want a specific tag such as a prerelease. The default
install directories are `$HOME/.local/bin` on macOS and Linux and
`%LOCALAPPDATA%\Programs\my-skills\bin` on Windows. Add the directory to PATH if
needed. In PowerShell, run `./scripts/install.ps1 -AddToPath` to update the user
PATH and the current PowerShell session.

## Quick start

After installing the CLI, run the following from outside the tool repo:

```bash
my-skills init-registry        # prompts for a location; default: ~/my-agent-skills
my-skills doctor               # confirm the Registry line points at your registry
my-skills install --dry-run    # preview seeded default skill installs
my-skills install              # deploy enabled skills into your agents
```

`init-registry` seeds the public-safe default skills, records the registry as
the active root, and initializes git unless you pass `--no-git`. After that,
commands work from any directory:

```bash
my-skills doctor
my-skills skills --json
```

## Updating the CLI

For a Rust build from source, pull the latest `dev` changes and rerun
`cargo install --locked --path .` from the tool repo. On `main`, the current
release is Python 0.4.2. Its `my-skills update` prints migration steps when a
Rust v1 release becomes available. See the 0.4.2 entry in
[CHANGELOG.md](CHANGELOG.md#042---2026-11-23).

The Rust CLI's `my-skills doctor` checks for a newer stable v1 release.
After a stable v1 release is published, run `my-skills update` to install it
if it is newer than your current version. The updater verifies the archive
checksum and replaces the executable you ran.

```bash
my-skills doctor
my-skills update
```

Use `--check` to check for an update or `--dry-run` to preview the downloads
and executable replacement for a newer release:

```bash
my-skills update --check
my-skills update --dry-run
```

Once v1 releases are published, `--channel main` can select a published
prerelease. It chooses the highest version among published releases:

```bash
my-skills update --channel main
```

`skills` shows every skill and where it's installed across your hosts:

```text
SKILL             ENABLED  CLAUDE   CODEX    HERMES
----------------  -------  -------  -------  -------
cli-inventory     yes      fresh    fresh    missing
my-skills         yes      fresh    fresh    fresh
```

## Public CLI, Private Registry

This repository has two jobs:

```text
github.com/Seosiju/my-skills
  public CLI package
  public-safe starter skills
  docs, tests, release automation

your private registry
  my-skills.toml
  skills/<name>/SKILL.md
  private workflows and personal/company skills

machine-local data path
  real config.json files
  accounts, tokens, memory, local state
```

Use the public repo to install the tool. Use a private registry when your
canonical skills contain personal, company, or machine-specific context. Keep
secrets out of both repos; put them under `my-skills data-path <skill>`.

Create a private registry. With no path, the default is `~/my-agent-skills`;
`init-registry` seeds the default public-safe skills, records the active root,
and runs `git init`:

```bash
my-skills init-registry
```

The scaffold looks like this:

```text
my-agent-skills/
├── my-skills.toml        # hosts, defaults, seeded [skills.<name>] entries
├── skills/               # canonical skills, including seeded defaults
├── .gitignore            # ignores local overrides
└── README.md             # private registry operating notes
```

Pass `--no-defaults` for a blank registry, or `--no-git` if you want a plain
folder with no git repository. On another machine, clone an existing registry
and run `my-skills bootstrap` from that clone to reconcile its host connections.
Run `my-skills hooks install` separately to set up its Git hooks.

## Where commands look for your registry

Most commands resolve a registry in this order:

1. `MY_SKILLS_ROOT` environment variable, for a one-command override.
2. `my-skills.toml` in the current directory or one of its parents, for that
   command only.
3. The cached active root written by `init-registry` or `set-root`.

If you run a command inside one registry while another valid active root is
cached, the command uses the current directory for that invocation and prints a
note to stderr; it does not silently repoint the active root. To switch
permanently, run:

```bash
my-skills set-root /path/to/your/registry
my-skills doctor
```

On a machine with no active root yet, first-run discovery from the current
directory can record that registry for convenience. Contributor clones also
contain seed `skills/` and `my-skills.toml` files for packaging and tests, so
use `my-skills doctor` and check the `Registry:` line before making real skill
changes.

Add your first private skill:

```bash
mkdir -p skills/my-private-skill
$EDITOR skills/my-private-skill/SKILL.md
```

Then register it in `my-skills.toml`:

```toml
[skills.my-private-skill]
enabled = true
hosts = ["claude", "codex", "hermes", "agy"]
```

Preview before writing to agent hosts:

```bash
my-skills validate
my-skills audit --all --json
my-skills install --dry-run
my-skills install
```

## Included skills

| Skill | What it does |
|-------|--------------|
| `cli-inventory` | Discover the CLI tools installed on this machine (PATH + Homebrew/npm/pipx/cargo/gem/pip) and record them to a machine-local inventory you can read back quickly. |
| `my-skills` | Agent-facing skill that guides catalog, share, install, and sync workflows through the CLI. |
| `my-jira` | Disabled-by-default Jira/Atlassian bootstrap skill with a machine-local config template. |

`personal-profile` is no longer bundled. The default set applies to newly
initialized registries. Updating the CLI package does not delete skills in a
separate personal registry, host installations, or machine-local data. Pulling
this tool repo does update its bundled seed sources; keep personal skills in a
separate registry.

If your registry still registers `personal-profile`, disable it before removing
host copies so a later `install` or `sync` does not restore them:

```bash
my-skills disable personal-profile
my-skills uninstall personal-profile --host <host>
```

If the skill is already unregistered, run only `uninstall`; it can remove
previously managed copies using the install state.

## Everyday commands

```bash
# See what exists and where it's installed.
my-skills skills              # add --json for agents/UIs
my-skills status              # install status per skill and host

# Install / update.
my-skills install --dry-run   # preview host changes
my-skills install             # enabled skills -> enabled hosts
my-skills install cli-inventory --host claude
my-skills install cli-inventory --host all --yes  # explicit multi-host write
my-skills sync --dry-run      # preview changes including removals
my-skills sync --yes          # reconcile the reviewed plan across enabled hosts
my-skills sync --check        # non-zero if reconciliation needs changes or is blocked

# Remove a managed install (recorded destinations only).
my-skills uninstall cli-inventory --host claude

# Turn a skill on/off in the manifest.
my-skills enable cli-inventory
my-skills disable cli-inventory
```

### Bring in a skill you already wrote

```bash
# Import an external skill directory into canonical skills/.
my-skills import ~/.hermes/skills/cli-inventory
my-skills enable cli-inventory
my-skills install cli-inventory --host <host>

# Or register it as enabled immediately.
my-skills import ~/.hermes/skills/cli-inventory --enable

# Or review a host's local skills, then promote one into my-skills.
my-skills share --from claude --plan --json
my-skills share --from claude cli-inventory --enable
my-skills sync cli-inventory --host claude
```

`import` registers the skill in `my-skills.toml` automatically. By default the
new entry is disabled; pass `--enable` or run `my-skills enable <skill>` before
installing it. Import sources that contain symlinks are rejected so the audit
and copy boundaries stay the same directory tree.

### Develop a skill live

```bash
# Link the host directory to the canonical skill so edits show up immediately.
my-skills install cli-inventory --host claude --mode link
```

Link mode uses a symlink on Unix and prefers a junction on Windows. Uninstalling a
managed link removes the link while leaving its target in the registry. Existing
installs retain their recorded mode. Use `my-skills convert --to link` with
`--dry-run` to preview converting existing copies.

## How it stays safe

- **Link by default.** Set `install_mode = "copy"` in the manifest or pass
  `--mode copy` to `install` to create new copies.
- **Multi-host writes require confirmation.** Review a dry run before running
  `install` or `sync` with `--yes` across multiple hosts. A plain `install` of
  only built-in seeds at default host paths is exempt.
- **Collision = block.** A pre-existing, unmanaged destination is never overwritten.
- **Drift-protected.** A locally edited copy is reported, not clobbered.
- **Atomic writes.** Installs stage to a temp dir and swap into place; failures roll back.

Install state is machine-local (`$XDG_STATE_HOME` or `~/.local/state/my-skills/`)
and never committed.

`sync` and `status` classify each (skill, host) so you always know what a write would do:

| State | Meaning |
|-------|---------|
| `FRESH` | install matches canonical |
| `STALE` | canonical changed since the copy was installed |
| `DRIFTED` | the install was edited locally; `sync` won't touch it |
| `CONFLICT` | both sides changed — no auto-merge |
| `MISSING` | registered but not installed |
| `UNMANAGED` | a destination exists without a managed install record |
| `BROKEN` | a managed link points at a missing canonical directory |
| `UNSUPPORTED` | the skill's host list excludes this host |

`sync` only writes the safe cases; `DRIFTED`, `CONFLICT`, and `UNMANAGED` block with a non-zero exit.

### Machine-local data

Canonical skills never store machine-specific data. Skills that need real local data
(e.g. `cli-inventory` scan results) read and write a single shared data root instead:

```bash
my-skills data-path cli-inventory          # resolve the path
my-skills data-path cli-inventory --create # and create it
```

The data root is machine-local and never committed. Skills that need private
configuration use the same pattern: keep only `config.example.json` in
`skills/<name>/`, then create the real `config.json` under that skill's data
path.

```bash
config_dir="$(my-skills data-path my-jira --create)"
cp skills/my-jira/config.example.json "$config_dir/config.json"
$EDITOR "$config_dir/config.json"
```

Keep machine-specific overrides in `my-skills.local.toml` or `local/`; both are
ignored by git. Do not commit host install copies, state files, account IDs,
tokens, or personal memory.

## Layout

### Repository layout (the CLI project)

```text
my-skills.toml            # dev/test manifest for this CLI project
skills/<name>/SKILL.md    # public seed sources embedded in the binary
Cargo.toml                # Rust package manifest
src/                      # Rust library and CLI
tests/                    # unit + fixture-driven tests
```

The `skills/` directory in this repository is seed source and a dev fixture for
tests/CI. It is not your registry.

### Your registry layout

```text
my-agent-skills/
├── my-skills.toml
├── skills/<name>/SKILL.md
├── .gitignore
└── README.md
```

By default, `install` connects enabled skills. A bare `sync` also considers
disabled and unregistered skills so it can remove their managed installs.
Use `sync --no-prune` to skip removals. Pass `--all` to `install` or `sync` to
include disabled skills among the connections to make.

## Tests

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo build --release
```

Run the CLI from the working tree. Use a test registry and test host paths for
smoke checks.

```bash
cargo run -- doctor --no-update-check
cargo run -- skills --json
cargo run -- bootstrap --dry-run
cargo run -- install my-skills --host hermes --dry-run
```

See [docs/release-checklist.md](docs/release-checklist.md) before tagging a
GitHub Release.

## License

[MIT](LICENSE)
