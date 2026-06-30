"""Host contract shared by every supported agent (plan section 13)."""

from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class HostConfig:
    """Declarative description of one agent host.

    Host-specific behavior is looked up through this registry rather than
    scattered as conditionals across the installer (plan section 13).
    """

    name: str
    display_name: str
    detect_commands: tuple[str, ...]
    default_user_path: str
    default_project_path: str
    supports_symlink: bool
    reload_hint: str
    frontmatter_policy: str = "standard"
    optional_metadata: tuple[str, ...] = ()
    description_max_chars: int = 768
    frontmatter_keep_fields: tuple[str, ...] = ("name", "description")
