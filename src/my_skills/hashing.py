"""Deterministic content hashing for skill directories (plan section 12 / 16.1).

A skill's identity is the set of (relative path, bytes) pairs it contains, so
the hash is independent of absolute location and of filesystem walk order.
"""

from __future__ import annotations

import hashlib
from pathlib import Path

from .ignore import is_ignored

_CHUNK = 65536


def hash_file(path: Path) -> str:
    """Return the hex SHA-256 of a single file's bytes."""
    h = hashlib.sha256()
    with Path(path).open("rb") as fh:
        for chunk in iter(lambda: fh.read(_CHUNK), b""):
            h.update(chunk)
    return h.hexdigest()


def hash_directory(path: Path) -> str:
    """Return a ``sha256:``-prefixed deterministic hash of a directory tree.

    Files are visited in sorted relative-path order; each contributes its
    relative POSIX path and its content hash, so renaming or editing any file
    changes the result while absolute location does not.
    """
    path = Path(path)
    h = hashlib.sha256()
    files = sorted(
        (
            p
            for p in path.rglob("*")
            if p.is_file() and not is_ignored(p.relative_to(path))
        ),
        key=lambda p: p.relative_to(path).as_posix(),
    )
    for file in files:
        rel = file.relative_to(path).as_posix()
        h.update(rel.encode("utf-8"))
        h.update(b"\0")
        h.update(hash_file(file).encode("ascii"))
        h.update(b"\0")
    return "sha256:" + h.hexdigest()
