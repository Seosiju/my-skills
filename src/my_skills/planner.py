"""Pure install/uninstall planning (plan section 12.1).

The planner computes, for each (skill, host), the action that *would* be taken,
from content hashes and recorded state. It never touches the filesystem — the
installer executes the plan.
"""

from __future__ import annotations

import os
from dataclasses import dataclass
from enum import Enum
from pathlib import Path

from .config import Manifest, Skill, Target
from .hashing import hash_directory
from .state import State


def is_managed_link(dest: Path, source: Path) -> bool:
    """True if *dest* is a symlink resolving to the canonical *source* dir."""
    try:
        return dest.is_symlink() and os.path.realpath(dest) == os.path.realpath(source)
    except OSError:
        return False


class Action(str, Enum):
    CREATE = "CREATE"
    UPDATE = "UPDATE"
    NOOP = "NOOP"
    REMOVE = "REMOVE"
    BLOCK_CONFLICT = "BLOCK_CONFLICT"
    BLOCK_DRIFT = "BLOCK_DRIFT"
    CONFLICT = "CONFLICT"
    SKIP_UNSUPPORTED = "SKIP_UNSUPPORTED"
    NOT_MANAGED = "NOT_MANAGED"


class Status(str, Enum):
    """Read-only drift state of one (skill, host) pair (plan section 9.7)."""

    FRESH = "FRESH"
    STALE = "STALE"
    DRIFTED = "DRIFTED"
    MISSING = "MISSING"
    CONFLICT = "CONFLICT"
    UNMANAGED = "UNMANAGED"
    UNSUPPORTED = "UNSUPPORTED"


@dataclass
class PlanItem:
    skill: str
    host: str
    source: Path
    destination: Path
    action: Action
    reason: str = ""
    source_hash: str | None = None
    mode: str = "copy"


def _destination(target: Target, skill_name: str) -> Path:
    return target.path / skill_name


def status_of(manifest: Manifest, skill: Skill, host: str, state: State) -> Status:
    """Read-only drift state for one (skill, host). Hashes but never writes."""
    if skill.hosts and host not in skill.hosts:
        return Status.UNSUPPORTED

    source = manifest.skills_dir / skill.name
    dest = _destination(manifest.targets[host], skill.name)
    record = state.get(skill.name, host)

    if record is None:
        return Status.UNMANAGED if dest.exists() else Status.MISSING

    if record.mode == "link":
        if is_managed_link(dest, source):
            return Status.FRESH
        if not dest.exists() and not dest.is_symlink():
            return Status.MISSING
        return Status.DRIFTED

    if not dest.exists():
        return Status.MISSING

    install_modified = hash_directory(dest) != record.installed_hash
    source_changed = hash_directory(source) != record.source_hash
    if install_modified and source_changed:
        return Status.CONFLICT
    if install_modified:
        return Status.DRIFTED
    if source_changed:
        return Status.STALE
    return Status.FRESH


def plan_install_one(
    manifest: Manifest,
    skill: Skill,
    host: str,
    target: Target,
    state: State,
    requested_mode: str = "copy",
) -> PlanItem:
    source = manifest.skills_dir / skill.name
    destination = _destination(target, skill.name)

    if skill.hosts and host not in skill.hosts:
        return PlanItem(
            skill.name, host, source, destination,
            Action.SKIP_UNSUPPORTED, reason=f"{host} not in skill.hosts",
        )

    source_hash = hash_directory(source)
    record = state.get(skill.name, host)
    # An existing install keeps its own mode; a fresh one uses the requested mode.
    mode = record.mode if record else requested_mode

    # Nothing at the destination (not a real dir, not a symlink) -> create/re-link.
    if not destination.exists() and not destination.is_symlink():
        return PlanItem(
            skill.name, host, source, destination,
            Action.CREATE, reason="destination missing",
            source_hash=source_hash, mode=mode,
        )

    if record is None:
        return PlanItem(
            skill.name, host, source, destination,
            Action.BLOCK_CONFLICT,
            reason="destination exists and is not managed by my-skills",
            source_hash=source_hash, mode=mode,
        )

    if record.mode == "link":
        if is_managed_link(destination, source):
            return PlanItem(
                skill.name, host, source, destination,
                Action.NOOP, reason="up to date (linked)",
                source_hash=source_hash, mode="link",
            )
        return PlanItem(
            skill.name, host, source, destination,
            Action.BLOCK_DRIFT, reason="managed link was replaced locally",
            source_hash=source_hash, mode="link",
        )

    install_modified = hash_directory(destination) != record.installed_hash
    source_changed = source_hash != record.source_hash

    if install_modified and source_changed:
        return PlanItem(
            skill.name, host, source, destination,
            Action.CONFLICT,
            reason="canonical and installed copy both changed (no auto-merge)",
            source_hash=source_hash, mode="copy",
        )
    if install_modified:
        return PlanItem(
            skill.name, host, source, destination,
            Action.BLOCK_DRIFT,
            reason="installed copy was modified locally",
            source_hash=source_hash, mode="copy",
        )
    if source_changed:
        return PlanItem(
            skill.name, host, source, destination,
            Action.UPDATE, reason="canonical changed",
            source_hash=source_hash, mode="copy",
        )
    return PlanItem(
        skill.name, host, source, destination,
        Action.NOOP, reason="up to date", source_hash=source_hash, mode="copy",
    )


def plan_install(
    manifest: Manifest,
    skills: list[Skill],
    hosts: list[str],
    state: State,
    requested_mode: str = "copy",
) -> list[PlanItem]:
    items: list[PlanItem] = []
    for skill in skills:
        for host in hosts:
            target = manifest.targets[host]
            items.append(
                plan_install_one(manifest, skill, host, target, state, requested_mode)
            )
    return items


def plan_uninstall_one(
    manifest: Manifest, skill_name: str, host: str, target: Target, state: State
) -> PlanItem:
    source = manifest.skills_dir / skill_name
    destination = _destination(target, skill_name)
    record = state.get(skill_name, host)

    if record is None:
        return PlanItem(
            skill_name, host, source, destination,
            Action.NOT_MANAGED, reason="not recorded as managed",
        )

    destination = Path(record.destination)

    if record.mode == "link":
        gone = not destination.exists() and not destination.is_symlink()
        if is_managed_link(destination, source) or gone:
            return PlanItem(
                skill_name, host, source, destination,
                Action.REMOVE, reason="managed link", mode="link",
            )
        return PlanItem(
            skill_name, host, source, destination,
            Action.BLOCK_DRIFT, reason="managed link was replaced locally",
            mode="link",
        )

    if destination.exists() and hash_directory(destination) != record.installed_hash:
        return PlanItem(
            skill_name, host, source, destination,
            Action.BLOCK_DRIFT, reason="installed copy was modified locally",
        )
    return PlanItem(
        skill_name, host, source, destination,
        Action.REMOVE, reason="managed install",
    )


def plan_uninstall(
    manifest: Manifest, skill_name: str, hosts: list[str], state: State
) -> list[PlanItem]:
    items: list[PlanItem] = []
    for host in hosts:
        target = manifest.targets[host]
        items.append(plan_uninstall_one(manifest, skill_name, host, target, state))
    return items
