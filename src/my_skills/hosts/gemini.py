"""Gemini CLI host configuration."""

from __future__ import annotations

from .base import HostConfig

GEMINI = HostConfig(
    name="gemini",
    display_name="Gemini CLI",
    detect_commands=("gemini",),
    default_user_path="~/.gemini/skills",
    default_project_path=".gemini/skills",
    supports_symlink=True,
    reload_hint="Gemini CLI loads skills at startup; restart to refresh.",
    frontmatter_policy="standard",
    optional_metadata=(),
)
