"""Codex host configuration."""

from __future__ import annotations

from .base import HostConfig

CODEX = HostConfig(
    name="codex",
    display_name="Codex",
    detect_commands=("codex",),
    default_user_path="~/.agents/skills",
    default_project_path=".agents/skills",
    supports_symlink=True,
    reload_hint="Codex detects skill changes automatically; restart if needed.",
    frontmatter_policy="standard",
    optional_metadata=(),
)
