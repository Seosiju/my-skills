# Host Connections

| Host | Default user skill directory |
| --- | --- |
| Claude Code | `~/.claude/skills` |
| Codex | `~/.agents/skills` |
| Antigravity CLI (`agy`) | `~/.gemini/config/skills` |

Links point to the canonical files and expose edits immediately. Copy mode tracks
changes on both sides and protects local edits. Set `install_mode = "link"` or
`"copy"` under `[targets.<host>]` in `my-skills.toml` or `my-skills.local.toml`.
New connections use `sync --mode`, then the configured target mode, then the default.

A configured copy mode can move a clean link to copy on sync. A copy moves to
link with `sync --mode link` or an explicit target link setting for a
non-fallback copy. Fallback copies keep refreshing as copies on bare sync.
Retry with `my-skills sync --host <host> --mode link` when link support becomes available.
Scope `--mode link` or `--mode copy` with `--host <host>` because it applies to every selected host, including Windows agy configured for copy.

## Antigravity CLI on Windows

AGY is opt-in for new registries. Set `enabled = true` under `[targets.agy]`
and include `agy` in the skill's hosts for default cross-host delivery.
Windows junctions may be invisible to the CLI. Merge this into
`my-skills.local.toml`.

```toml
[targets.agy]
install_mode = "copy"
```

Run `my-skills sync --host agy --dry-run --json`, then
`my-skills sync --host agy` after reviewing the plan.
Check discovery from PowerShell with `agy -p '/skills'` because Git Bash rewrites `/skills` as a filesystem path.

## Diagnose

`my-skills doctor --quick --json` shows registry selection, host settings,
catalog description counts, and connection counts. For complete local
diagnostics without checking for an update, run `my-skills doctor --no-update-check`.
