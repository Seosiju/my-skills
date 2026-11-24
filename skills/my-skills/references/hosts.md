# Hosts, Install Modes, And Antigravity CLI (agy)

## Link vs copy and `install_mode` precedence

Host directories are `~/.claude/skills` (Claude Code), `~/.agents/skills`
(Codex), and `~/.gemini/config/skills` (Antigravity CLI). New registries use
links by default so edits to canonical skills are visible immediately. In copy
mode, the CLI detects local changes before replacing a copy.

You can set `install_mode = "link" | "copy"` per host under `[targets.<host>]`
in `my-skills.toml` or `my-skills.local.toml`. Precedence for new connections:

1. `install --mode`
2. `[targets.<host>] install_mode`
3. `[defaults] install_mode`

Existing connections keep their recorded mode until converted with `convert`.

## Quick host diagnostics

Run `my-skills doctor --quick --json` to see the registry, host settings, and
connection counts. Quick mode skips the update lookup, Git and hook diagnostics,
full validation, and the link probe.

Each `hosts` row reports `configured_mode` (`link` or `copy` for new
connections, or `null` when the manifest is unavailable). The `connections`
counts cover records owned by the displayed registry. A copy fallback is
included in both the `copy` and `fallback` counts. For full local diagnostics
without the update lookup, use `--no-update-check` or
`MY_SKILLS_NO_UPDATE_CHECK=1`.

## Antigravity CLI (agy)

AGY is a supported, opt-in host. Set `enabled = true` under `[targets.agy]` and
include `agy` in the desired skill's `hosts` list to add it to default
cross-host operations. An omitted target is disabled; an explicit `--host agy`
still selects it, following the same explicit-host rules as other agents.

- **Paths:** user-scope `~/.gemini/config/skills`; project-scope `.agents/skills`.
- **Platform notes:** copy and symlink discovery were verified on macOS with
  Antigravity CLI `1.2.16`. On Windows, `my-skills` links with junctions by
  default, which Antigravity CLI `1.5.14` did not discover; Windows users can
  set `[targets.agy] install_mode = "copy"` in `my-skills.local.toml` so `agy`
  uses copies while other hosts keep links (`convert --to link --host all` skips
  `agy` automatically, while `--host agy` still converts as requested). This
  does not establish Antigravity IDE or other-version compatibility.
- **Verify discovery:** after installation, start a new CLI session and use
  `/skills` to check discovery (on Windows, run
  `agy -p '/skills' --output-format json` from PowerShell rather than Git Bash,
  which rewrites `/skills` into a Windows path):

```bash
my-skills install <skill> --host agy --dry-run --json
my-skills skills --host agy --json
agy -p '/skills' --output-format json
```

Preserve unmanaged AGY skills: compare and back them up outside its discovery
path before migration. Do not force replacement or bypass audit automatically.
