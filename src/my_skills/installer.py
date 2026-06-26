"""Copy-mode install and uninstall execution (plan section 12.2).

Installs stage into a temporary sibling directory and are swapped into place
with ``os.replace`` so a failure never leaves a half-written destination. An
existing managed copy is moved aside first and only deleted once the new copy
is in place; on failure the original is restored.
"""

from __future__ import annotations

import os
import shutil
import tempfile
from datetime import datetime, timezone
from pathlib import Path

from .hashing import hash_directory
from .planner import PlanItem
from .state import InstallRecord


def _utcnow() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def copy_install(item: PlanItem, mode: str = "copy") -> InstallRecord:
    """Install ``item.source`` to ``item.destination`` by atomic directory swap."""
    source = Path(item.source)
    dest = Path(item.destination)
    if not source.is_dir():
        raise FileNotFoundError(f"source skill directory not found: {source}")
    dest.parent.mkdir(parents=True, exist_ok=True)

    staging = Path(tempfile.mkdtemp(prefix=f".{dest.name}.staging-", dir=dest.parent))
    staged = staging / dest.name
    backup: Path | None = None
    try:
        shutil.copytree(source, staged)

        if dest.exists():
            backup = dest.parent / f".{dest.name}.backup-{os.getpid()}"
            if backup.exists():
                shutil.rmtree(backup)
            os.replace(dest, backup)

        try:
            os.replace(staged, dest)
        except OSError:
            if backup is not None:
                os.replace(backup, dest)  # restore original
                backup = None
            raise
    finally:
        shutil.rmtree(staging, ignore_errors=True)
        if backup is not None and backup.exists():
            shutil.rmtree(backup, ignore_errors=True)

    source_hash = item.source_hash or hash_directory(source)
    return InstallRecord(
        skill=item.skill,
        host=item.host,
        mode=mode,
        source=str(source),
        destination=str(dest),
        source_hash=source_hash,
        installed_hash=hash_directory(dest),
        installed_at=_utcnow(),
    )


def uninstall(record_destination: str | Path) -> None:
    """Remove a managed install destination directory if it exists."""
    dest = Path(record_destination)
    if dest.exists():
        shutil.rmtree(dest)
