from __future__ import annotations

from pathlib import Path
from typing import Final


IGNORED_DIRS: Final = frozenset({".omc", ".git", "__pycache__"})
IGNORED_FILES: Final = frozenset({".DS_Store"})


def is_ignored(rel: Path) -> bool:
    if rel.name in IGNORED_FILES:
        return True
    return any(part in IGNORED_DIRS for part in rel.parts)
