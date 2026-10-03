"""Antigravity CLI host configuration (discovery verified with CLI 1.2.16)."""

from __future__ import annotations

from .base import HostConfig

AGY = HostConfig(
    name="agy",
    display_name="Antigravity CLI",
    detect_commands=("agy",),
    default_user_path="~/.gemini/config/skills",
    default_project_path=".agents/skills",
    supports_symlink=True,
    reload_hint="Start a new Antigravity CLI session and use /skills to verify discovery.",
    default_enabled=False,
)
