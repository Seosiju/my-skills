from __future__ import annotations

import argparse
import json
import sys
from dataclasses import replace
from typing import Any, NotRequired, TypedDict

from .audit.formatting import format_gate, gate_json
from .audit.gate import audit_metadata, audit_policy_from_manifest, audit_skills
from .checks import compose_validation
from .cli_runtime import load_manifest_from_cwd, resolve_hosts, select_requested
from .config import Manifest, ManifestError, Skill
from .hosts import get_host
from .installer import copy_install, link_install, uninstall
from .planner import Action, PlanItem, Status, plan_install, plan_uninstall, status_of
from .state import State, StateError
from .validation import validate_skill_for_host
from .write_confirmation import confirm_multi_host_write, is_builtin_seed_default_install


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
    audit: NotRequired[dict[str, Any]]


def _validate_selected(manifest: Manifest, skills: list[Skill], hosts: list[str]) -> bool:
    ok = True
    for skill in skills:
        skill_dir = manifest.skills_dir / skill.name
        result = compose_validation(skill_dir)
        if not result.ok:
            ok = False
            print(f"[BLOCKED] {skill.name}: validation failed")
            for error in result.errors:
                print(f"  error: {error}")
        for host in hosts:
            try:
                host_config = get_host(host)
            except KeyError:
                continue
            host_result = validate_skill_for_host(skill_dir, host_config)
            if not host_result.ok:
                ok = False
                print(f"[BLOCKED] {skill.name}: {host} validation failed")
                for error in host_result.errors:
                    print(f"  error: {error}")
            for warning in host_result.warnings:
                print(f"  warn: {warning}")
    return ok


_BLOCK_ACTIONS = (Action.BLOCK_CONFLICT, Action.BLOCK_DRIFT, Action.CONFLICT)


def _apply_plan(plan: list[PlanItem], state: State) -> tuple[int, bool]:
    return _apply_plan_with_audit(plan, state, {})


def _apply_plan_with_audit(
    plan: list[PlanItem],
    state: State,
    audit_by_skill: dict[str, dict[str, str]],
) -> tuple[int, bool]:
    changed = 0
    blocked = False
    for item in plan:
        if item.action in (Action.CREATE, Action.UPDATE):
            try:
                if item.mode == "link":
                    record = link_install(item)
                else:
                    record = copy_install(item, mode=item.mode)
            except OSError as exc:
                blocked = True
                print(f"  FAILED: {item.skill} -> {item.host} ({exc})")
                continue
            metadata = audit_by_skill.get(item.skill)
            if metadata:
                record = replace(record, **metadata)
            state.put(record)
            state.save()
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


def _audit_selected(manifest: Manifest, skills: list[Skill], *, skip: bool):
    paths = tuple(manifest.skills_dir / skill.name for skill in skills)
    return audit_skills(paths, policy=audit_policy_from_manifest(manifest), skip=skip)


def _audit_by_skill(gate) -> dict[str, dict[str, str]]:
    return {result.skill: audit_metadata(result) for result in gate.results}


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

    if not _validate_selected(manifest, skills, hosts):
        print("\nNo files were changed (fix validation errors first).", file=sys.stderr)
        return 1

    gate = _audit_selected(manifest, skills, skip=args.skip_audit)
    try:
        state = State.load()
    except StateError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2
    plan = plan_install(manifest, skills, hosts, state, requested_mode=args.mode)

    if args.dry_run:
        if args.json:
            payload = _install_plan_json(plan)
            payload["audit"] = gate_json(gate)
            print(json.dumps(payload, indent=2))
            return 0
        print(format_gate(gate, blocked_title="AUDIT WOULD BLOCK"))
        print("Dry run — planned actions (nothing written):")
        for item in plan:
            tag = f" [{item.mode}]" if item.mode == "link" else ""
            print(
                f"  {item.action.value:16} {item.skill} -> {item.host}{tag}  "
                f"({item.reason})"
            )
        return 0

    if not confirm_multi_host_write(
        args,
        hosts,
        allow_without_yes=is_builtin_seed_default_install(
            args,
            manifest,
            skills,
            hosts,
        ),
    ):
        return 2

    if gate.skipped:
        print(format_gate(gate))
    elif gate.blocked:
        print(format_gate(gate))
        print("\nNo files were changed (audit blocked).", file=sys.stderr)
        return 1

    changed, blocked = _apply_plan_with_audit(plan, state, _audit_by_skill(gate))
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

    try:
        state = State.load()
    except StateError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2

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

    if not confirm_multi_host_write(args, hosts):
        return 2

    if not _validate_selected(manifest, skills, hosts):
        print("\nNo files were changed (fix validation errors first).", file=sys.stderr)
        return 1

    gate = _audit_selected(manifest, skills, skip=args.skip_audit)
    if gate.skipped:
        print(format_gate(gate))
    elif gate.blocked:
        print(format_gate(gate))
        print("\nNo files were changed (audit blocked).", file=sys.stderr)
        return 1

    plan = plan_install(manifest, skills, hosts, state)
    changed, blocked = _apply_plan_with_audit(plan, state, _audit_by_skill(gate))
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

    try:
        state = State.load()
    except StateError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2
    plan = plan_uninstall(manifest, args.skill, hosts, state)
    if not confirm_multi_host_write(args, hosts):
        return 2

    removed = 0
    warned = False
    failed = False
    for item in plan:
        if item.action is Action.REMOVE:
            try:
                uninstall(item.destination)
            except OSError as exc:
                failed = True
                print(f"  FAILED: {item.skill} -> {item.host} ({exc})")
                continue
            state.remove(item.skill, item.host)
            state.save()
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
    return 1 if warned or failed else 0
