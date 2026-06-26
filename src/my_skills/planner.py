"""Pure install/uninstall planning (plan section 12.1).

The planner computes, for each (skill, host), the action that *would* be taken,
from content hashes and recorded state. It never touches the filesystem — the
installer executes the plan.
"""

from __future__ import annotations

from dataclasses import dataclass
from enum import Enum
from pathlib import Path

from .config import Manifest, Skill, Target
from .hashing import hash_directory
from .state import State


class Action(str, Enum):
    CREATE = "CREATE"
    UPDATE = "UPDATE"
    NOOP = "NOOP"
    REMOVE = "REMOVE"
    BLOCK_CONFLICT = "BLOCK_CONFLICT"
    BLOCK_DRIFT = "BLOCK_DRIFT"
    SKIP_UNSUPPORTED = "SKIP_UNSUPPORTED"
    NOT_MANAGED = "NOT_MANAGED"


@dataclass
class PlanItem:
    skill: str
    host: str
    source: Path
    destination: Path
    action: Action
    reason: str = ""
    source_hash: str | None = None


def _destination(target: Target, skill_name: str) -> Path:
    return target.path / skill_name


def plan_install_one(
    manifest: Manifest, skill: Skill, host: str, target: Target, state: State
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

    if not destination.exists():
        return PlanItem(
            skill.name, host, source, destination,
            Action.CREATE, reason="destination missing", source_hash=source_hash,
        )

    if record is None:
        return PlanItem(
            skill.name, host, source, destination,
            Action.BLOCK_CONFLICT,
            reason="destination exists and is not managed by my-skills",
            source_hash=source_hash,
        )

    installed_now = hash_directory(destination)
    if installed_now != record.installed_hash:
        return PlanItem(
            skill.name, host, source, destination,
            Action.BLOCK_DRIFT,
            reason="installed copy was modified locally", source_hash=source_hash,
        )
    if source_hash != record.source_hash:
        return PlanItem(
            skill.name, host, source, destination,
            Action.UPDATE, reason="canonical changed", source_hash=source_hash,
        )
    return PlanItem(
        skill.name, host, source, destination,
        Action.NOOP, reason="up to date", source_hash=source_hash,
    )


def plan_install(
    manifest: Manifest, skills: list[Skill], hosts: list[str], state: State
) -> list[PlanItem]:
    items: list[PlanItem] = []
    for skill in skills:
        for host in hosts:
            target = manifest.targets[host]
            items.append(plan_install_one(manifest, skill, host, target, state))
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
