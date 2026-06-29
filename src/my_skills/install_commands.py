from __future__ import annotations

import argparse
import json
import sys
from typing import TypedDict

from .checks import compose_validation
from .cli_runtime import load_manifest_from_cwd, resolve_hosts, select_requested
from .config import Manifest, ManifestError, Skill
from .installer import copy_install, link_install, uninstall
from .planner import Action, PlanItem, Status, plan_install, plan_uninstall, status_of
from .state import State


class InstallActionJson(TypedDict):
    skill: str
    host: str
    action: str
    reason: str
    mode: str
    source: str
    destination: str


class InstallPlanJson(TypedDict):
    actions: list[InstallActionJson]


def _validate_selected(manifest: Manifest, skills: list[Skill]) -> bool:
    ok = True
    for skill in skills:
        result = compose_validation(manifest.skills_dir / skill.name)
        if not result.ok:
            ok = False
            print(f"[BLOCKED] {skill.name}: validation failed")
            for error in result.errors:
                print(f"  error: {error}")
    return ok


_BLOCK_ACTIONS = (Action.BLOCK_CONFLICT, Action.BLOCK_DRIFT, Action.CONFLICT)


def _apply_plan(plan: list[PlanItem], state: State) -> tuple[int, bool]:
    changed = 0
    blocked = False
    for item in plan:
        if item.action in (Action.CREATE, Action.UPDATE):
            if item.mode == "link":
                state.put(link_install(item))
            else:
                state.put(copy_install(item, mode=item.mode))
            changed += 1
            verb = "created" if item.action is Action.CREATE else "updated"
            suffix = " (link)" if item.mode == "link" else ""
            print(f"  {verb}: {item.skill} -> {item.host}{suffix}")
        elif item.action is Action.NOOP:
            print(f"  unchanged: {item.skill} -> {item.host}")
        elif item.action is Action.SKIP_UNSUPPORTED:
            print(f"  skipped: {item.skill} -> {item.host} ({item.reason})")
        elif item.action in _BLOCK_ACTIONS:
            blocked = True
            print(f"  BLOCKED: {item.skill} -> {item.host}")
            print(f"    {item.reason}:")
            print(f"    {item.destination}")
    return changed, blocked


def _install_plan_json(plan: list[PlanItem]) -> InstallPlanJson:
    return {
        "actions": [
            {
                "skill": item.skill,
                "host": item.host,
                "action": item.action.value,
                "reason": item.reason,
                "mode": item.mode,
                "source": str(item.source),
                "destination": str(item.destination),
            }
            for item in plan
        ]
    }


def cmd_install(args: argparse.Namespace) -> int:
    try:
        manifest = load_manifest_from_cwd()
    except ManifestError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2

    try:
        skills, hosts = select_requested(args, manifest)
    except ManifestError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2

    if args.json and not args.dry_run:
        print("error: install --json requires --dry-run", file=sys.stderr)
        return 2

    if not _validate_selected(manifest, skills):
        print("\nNo files were changed (fix validation errors first).", file=sys.stderr)
        return 1

    state = State.load()
    plan = plan_install(manifest, skills, hosts, state, requested_mode=args.mode)

    if args.dry_run:
        if args.json:
            print(json.dumps(_install_plan_json(plan), indent=2))
            return 0
        print("Dry run — planned actions (nothing written):")
        for item in plan:
            tag = f" [{item.mode}]" if item.mode == "link" else ""
            print(
                f"  {item.action.value:16} {item.skill} -> {item.host}{tag}  "
                f"({item.reason})"
            )
        return 0

    changed, blocked = _apply_plan(plan, state)
    if changed:
        state.save()
    return 1 if blocked else 0


def cmd_sync(args: argparse.Namespace) -> int:
    try:
        manifest = load_manifest_from_cwd()
    except ManifestError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2

    try:
        skills, hosts = select_requested(args, manifest)
    except ManifestError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2

    state = State.load()

    if args.check:
        drift = False
        for skill in skills:
            print(skill.name)
            for host in hosts:
                status = status_of(manifest, skill, host, state)
                print(f"  {host:8} {status.value}")
                if status not in (Status.FRESH, Status.UNSUPPORTED):
                    drift = True
        return 1 if drift else 0

    if not _validate_selected(manifest, skills):
        print("\nNo files were changed (fix validation errors first).", file=sys.stderr)
        return 1

    plan = plan_install(manifest, skills, hosts, state)
    changed, blocked = _apply_plan(plan, state)
    if changed:
        state.save()
    return 1 if blocked else 0


def cmd_uninstall(args: argparse.Namespace) -> int:
    try:
        manifest = load_manifest_from_cwd()
    except ManifestError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2

    if not args.skill:
        print("error: uninstall requires a skill name", file=sys.stderr)
        return 2

    try:
        hosts = resolve_hosts(manifest, args.host)
    except ManifestError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2

    state = State.load()
    plan = plan_uninstall(manifest, args.skill, hosts, state)
    removed = 0
    warned = False
    for item in plan:
        if item.action is Action.REMOVE:
            uninstall(item.destination)
            state.remove(item.skill, item.host)
            removed += 1
            print(f"  removed: {item.skill} -> {item.host}")
        elif item.action is Action.BLOCK_DRIFT:
            warned = True
            print(f"  WARN: {item.skill} -> {item.host} was modified locally; not removed")
            print(f"    {item.destination}")
        elif item.action is Action.NOT_MANAGED:
            print(f"  skipped: {item.skill} -> {item.host} (not managed by my-skills)")
    if removed:
        state.save()
    return 1 if warned else 0
