"""Command-line interface for my-skills.

Phase 1: ``validate``, ``doctor``.
Phase 2: ``install`` (with ``--dry-run``), ``status``, ``uninstall``.
"""

from __future__ import annotations

import argparse
import os
import platform
import shutil
import sys
from pathlib import Path

from . import config as config_mod
from .config import Manifest, ManifestError, Skill, load_manifest, selected_skills
from .hashing import hash_directory
from .hosts import all_hosts
from .installer import copy_install, uninstall
from .planner import Action, plan_install, plan_uninstall
from .security import scan_skill
from .state import State
from .validation import ValidationResult, validate_skill


def find_repo_root(start: Path | None = None) -> Path:
    """Walk upward from *start* to the directory containing ``my-skills.toml``."""
    start = (start or Path.cwd()).resolve()
    for directory in (start, *start.parents):
        if (directory / "my-skills.toml").is_file():
            return directory
    raise ManifestError(
        "my-skills.toml not found in the current directory or any parent"
    )


def compose_validation(skill_dir: Path) -> ValidationResult:
    """Run structural validation and the security scan, merged by severity."""
    result = validate_skill(skill_dir)
    for finding in scan_skill(skill_dir):
        line = f"security: {finding.file}: {finding.message}"
        if finding.severity == "error":
            result.errors.append(line)
        else:
            result.warnings.append(line)
    return result


def _resolve_hosts(manifest: Manifest, host_arg: str | None) -> list[str]:
    if host_arg and host_arg != "all":
        if host_arg not in manifest.targets:
            raise ManifestError(f"unknown host: {host_arg}")
        return [host_arg]
    return [name for name, target in manifest.targets.items() if target.enabled]


def _load(args: argparse.Namespace) -> Manifest:
    return load_manifest(find_repo_root())


# ---------------------------------------------------------------- validate ---


def _skill_dirs(manifest: Manifest, skill: str | None) -> list[Path]:
    skills_dir = manifest.skills_dir
    if skill:
        return [skills_dir / skill]
    if not skills_dir.is_dir():
        return []
    return sorted(p for p in skills_dir.iterdir() if p.is_dir())


def cmd_validate(args: argparse.Namespace) -> int:
    try:
        manifest = _load(args)
    except ManifestError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2

    dirs = _skill_dirs(manifest, args.skill)
    if not dirs:
        print("no skills found to validate")
        return 0

    had_error = False
    for directory in dirs:
        result = compose_validation(directory)
        print(f"[{'OK' if result.ok else 'FAIL'}] {directory.name}")
        for error in result.errors:
            print(f"  error: {error}")
            had_error = True
        for warning in result.warnings:
            print(f"  warn:  {warning}")
    return 1 if had_error else 0


# ------------------------------------------------------------------ doctor ---


def _writable(path: Path) -> bool:
    probe = path
    while not probe.exists() and probe != probe.parent:
        probe = probe.parent
    return os.access(probe, os.W_OK)


def cmd_doctor(args: argparse.Namespace) -> int:
    print(f"OS:     {platform.platform()}")
    print(f"Shell:  {os.environ.get('SHELL', 'unknown')}")
    print(f"Python: {platform.python_version()}")

    manifest = None
    manifest_error = None
    try:
        manifest = _load(args)
    except ManifestError as exc:
        manifest_error = exc

    print("\nHosts:")
    for host in all_hosts():
        exe = next((c for c in host.detect_commands if shutil.which(c)), None)
        detected = f"found ({exe})" if exe else "not found"
        if manifest and host.name in manifest.targets:
            target = manifest.targets[host.name]
            path, enabled = target.path, target.enabled
        else:
            path, enabled = config_mod.expand_path(host.default_user_path), True
        print(
            f"  {host.display_name:12} exe={detected:18} "
            f"enabled={enabled!s:5} writable={_writable(path)!s:5} path={path}"
        )

    if manifest_error is None:
        print("\nManifest: valid")
        return 0
    print(f"\nManifest: INVALID ({manifest_error})")
    return 1


# ----------------------------------------------------------------- install ---


def _select(args: argparse.Namespace, manifest: Manifest) -> tuple[list[Skill], list[str]]:
    skills = selected_skills(
        manifest,
        explicit=[args.skill] if getattr(args, "skill", None) else None,
        all=getattr(args, "all", False),
    )
    hosts = _resolve_hosts(manifest, getattr(args, "host", None))
    return skills, hosts


def cmd_install(args: argparse.Namespace) -> int:
    try:
        manifest = _load(args)
    except ManifestError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2

    if args.mode == "link":
        print(
            "error: --mode link is not supported yet (planned for a later phase); "
            "use the default copy mode",
            file=sys.stderr,
        )
        return 2

    try:
        skills, hosts = _select(args, manifest)
    except ManifestError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2

    # Validate every selected skill before touching any host.
    invalid = False
    for skill in skills:
        result = compose_validation(manifest.skills_dir / skill.name)
        if not result.ok:
            invalid = True
            print(f"[BLOCKED] {skill.name}: validation failed")
            for error in result.errors:
                print(f"  error: {error}")
    if invalid:
        print("\nNo files were changed (fix validation errors first).", file=sys.stderr)
        return 1

    state = State.load()
    plan = plan_install(manifest, skills, hosts, state)

    if args.dry_run:
        print("Dry run — planned actions (nothing written):")
        for item in plan:
            print(f"  {item.action.value:16} {item.skill} -> {item.host}  ({item.reason})")
        return 0

    blocked = False
    changed = 0
    for item in plan:
        if item.action in (Action.CREATE, Action.UPDATE):
            state.put(copy_install(item, mode=args.mode))
            changed += 1
            verb = "created" if item.action is Action.CREATE else "updated"
            print(f"  {verb}: {item.skill} -> {item.host}")
        elif item.action is Action.NOOP:
            print(f"  unchanged: {item.skill} -> {item.host}")
        elif item.action is Action.SKIP_UNSUPPORTED:
            print(f"  skipped: {item.skill} -> {item.host} ({item.reason})")
        elif item.action in (Action.BLOCK_CONFLICT, Action.BLOCK_DRIFT):
            blocked = True
            print(f"  BLOCKED: {item.skill} -> {item.host}")
            print(f"    {item.reason}:")
            print(f"    {item.destination}")
    if changed:
        state.save()
    return 1 if blocked else 0


# ------------------------------------------------------------------ status ---


def _status_for(manifest: Manifest, skill: Skill, host: str, state: State) -> str:
    if skill.hosts and host not in skill.hosts:
        return "UNSUPPORTED"
    dest = manifest.targets[host].path / skill.name
    record = state.get(skill.name, host)
    if record is None:
        return "CONFLICT  (unmanaged copy present)" if dest.exists() else "MISSING"
    if not dest.exists():
        return "MISSING   (recorded but absent)"
    if hash_directory(dest) != record.installed_hash:
        return "DRIFTED   (installed copy modified)"
    if hash_directory(manifest.skills_dir / skill.name) != record.source_hash:
        return "STALE     (canonical changed)"
    return "FRESH"


def cmd_status(args: argparse.Namespace) -> int:
    try:
        manifest = _load(args)
    except ManifestError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2

    state = State.load()
    hosts = [name for name, target in manifest.targets.items() if target.enabled]
    if not manifest.skills:
        print("no skills registered in manifest")
        return 0
    for skill in manifest.skills.values():
        print(skill.name)
        for host in hosts:
            print(f"  {host:8} {_status_for(manifest, skill, host, state)}")
    return 0


# --------------------------------------------------------------- uninstall ---


def cmd_uninstall(args: argparse.Namespace) -> int:
    try:
        manifest = _load(args)
    except ManifestError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2

    if not args.skill:
        print("error: uninstall requires a skill name", file=sys.stderr)
        return 2

    try:
        hosts = _resolve_hosts(manifest, args.host)
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


# -------------------------------------------------------------------- main ---


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="my-skills",
        description="Personal cross-agent Agent Skill registry.",
    )
    sub = parser.add_subparsers(dest="command")

    p_validate = sub.add_parser(
        "validate", help="Validate canonical skills against the Agent Skills standard"
    )
    p_validate.add_argument("skill", nargs="?", help="Skill name (default: all)")
    p_validate.set_defaults(func=cmd_validate)

    p_doctor = sub.add_parser(
        "doctor", help="Report environment, hosts, and manifest health"
    )
    p_doctor.set_defaults(func=cmd_doctor)

    p_install = sub.add_parser("install", help="Install skills into host directories (copy mode)")
    p_install.add_argument("skill", nargs="?", help="Skill name (default: enabled skills)")
    p_install.add_argument("--host", help="Target host name, or 'all' (default: enabled targets)")
    p_install.add_argument("--all", action="store_true", help="Include every registered skill")
    p_install.add_argument("--mode", choices=("copy", "link"), default="copy", help="Install mode")
    p_install.add_argument("--dry-run", action="store_true", help="Show the plan; change nothing")
    p_install.set_defaults(func=cmd_install)

    p_status = sub.add_parser("status", help="Show install status per skill and host")
    p_status.set_defaults(func=cmd_status)

    p_uninstall = sub.add_parser("uninstall", help="Remove managed installs")
    p_uninstall.add_argument("skill", nargs="?", help="Skill name")
    p_uninstall.add_argument("--host", help="Target host name, or 'all' (default: enabled targets)")
    p_uninstall.set_defaults(func=cmd_uninstall)

    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    if not getattr(args, "command", None):
        parser.print_help()
        return 0
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main())
