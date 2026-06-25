"""Hermes host configuration."""

from __future__ import annotations

from .base import HostConfig

HERMES = HostConfig(
    name="hermes",
    display_name="Hermes",
    detect_commands=("hermes",),
    default_user_path="~/.hermes/skills",
    default_project_path=".hermes/skills",
    supports_symlink=True,
    reload_hint="Hermes reloads skills on restart; the agent may also edit them.",
    frontmatter_policy="standard",
    optional_metadata=(),
)
