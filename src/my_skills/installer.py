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
from .ignore import IGNORED_DIRS, IGNORED_FILES
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
        shutil.copytree(
            source,
            staged,
            ignore=shutil.ignore_patterns(*IGNORED_DIRS, *IGNORED_FILES),
        )

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


def link_install(item: PlanItem) -> InstallRecord:
    """Install ``item.source`` to ``item.destination`` as a directory symlink.

    Development mode (plan 12.3): the destination points at the canonical skill
    so edits are reflected immediately. Symlink creation failures are surfaced,
    never silently downgraded to a copy.
    """
    source = Path(item.source)
    dest = Path(item.destination)
    if not source.is_dir():
        raise FileNotFoundError(f"source skill directory not found: {source}")
    dest.parent.mkdir(parents=True, exist_ok=True)

    # Re-linking: drop a previous managed link first (never touch its target).
    if dest.is_symlink():
        dest.unlink()

    target = source.resolve()
    try:
        dest.symlink_to(target, target_is_directory=True)
    except OSError as exc:
        raise OSError(
            f"cannot create a symlink at {dest}: {exc}. Link mode requires "
            "symlink support (on Windows, enable Developer Mode or run elevated). "
            "Not falling back to copy silently — use the default copy mode instead."
        ) from exc

    source_hash = item.source_hash or hash_directory(source)
    return InstallRecord(
        skill=item.skill,
        host=item.host,
        mode="link",
        source=str(source),
        destination=str(dest),
        # A link's installed content IS the canonical content.
        source_hash=source_hash,
        installed_hash=source_hash,
        installed_at=_utcnow(),
    )


def uninstall(record_destination: str | Path) -> None:
    """Remove a managed install destination.

    A symlink is unlinked (so the canonical source it points at is never
    deleted); a real directory is removed recursively.
    """
    dest = Path(record_destination)
    if dest.is_symlink():
        dest.unlink()
    elif dest.exists():
        shutil.rmtree(dest)
