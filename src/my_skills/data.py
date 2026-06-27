"""Shared per-skill data root (plan section 15.4).

Skills that hold real, machine-local data (for example ``personal-profile``'s
memory) need ONE location that every host shares. Canonical skills are *copied*
out to each host directory on ``sync`` (``~/.claude/skills/<skill>``,
``~/.agents/skills/<skill>``, ``~/.hermes/skills/<skill>``), so a copy-relative
``local/`` would diverge per host and never converge. This module resolves a
single machine-level data root, keyed by skill name, that all those copies can
point at via ``my-skills data-path <skill>``.

The data root is machine-local and never committed. It is the one sanctioned
exception to skill host-neutrality (plan 5.3): the path belongs to ``my-skills``
itself, not to any host.
"""

from __future__ import annotations

import os
import sys
from pathlib import Path

from .validation import validate_name

APP = "my-skills"


def data_root() -> Path:
    """Resolve the machine-level base directory for all my-skills skill data.

    POSIX:   ``$XDG_DATA_HOME/my-skills`` (fallback ``~/.local/share/my-skills``)
    Windows: ``%LOCALAPPDATA%\\my-skills\\data`` (fallback ``~/AppData/Local/...``)
    """
    if sys.platform == "win32":
        base = os.environ.get("LOCALAPPDATA")
        root = Path(base) if base else Path.home() / "AppData" / "Local"
        return root / APP / "data"
    xdg = os.environ.get("XDG_DATA_HOME")
    base = Path(xdg) if xdg else Path.home() / ".local" / "share"
    return base / APP


def skill_data_path(skill: str, *, create: bool = False) -> Path:
    """Return the shared data directory for *skill*.

    The skill name is validated against the Agent Skills name rule, which only
    permits ``[a-z0-9]`` words joined by single hyphens — this also rejects any
    path-traversal attempt (``/``, ``\\``, ``..``, empty). With ``create=True``
    the directory is created (``parents=True, exist_ok=True``); otherwise the
    filesystem is left untouched.
    """
    errors = validate_name(skill)
    if errors:
        raise ValueError(f"invalid skill name {skill!r}: {'; '.join(errors)}")
    path = data_root() / skill
    if create:
        path.mkdir(parents=True, exist_ok=True)
    return path
