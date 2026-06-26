"""Claude Code host configuration."""

from __future__ import annotations

from .base import HostConfig

CLAUDE = HostConfig(
    name="claude",
    display_name="Claude Code",
    detect_commands=("claude",),
    default_user_path="~/.claude/skills",
    default_project_path=".claude/skills",
    supports_symlink=True,
    reload_hint="Claude Code picks up skills on restart; relaunch to see new skills.",
    frontmatter_policy="standard",
    optional_metadata=(),
)
