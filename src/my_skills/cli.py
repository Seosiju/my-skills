"""Command-line interface for my-skills.

Phase 1: ``validate``, ``doctor``.
Phase 2: ``install`` (with ``--dry-run``), ``status``, ``uninstall``.
Phase 3: ``sync`` (with ``--check``).
Phase 5.5: ``data-path`` (resolve a skill's shared machine-local data dir).
Phase 6: ``import`` (host skill -> canonical) and ``install --mode link``.
"""

from __future__ import annotations

import argparse
import json
import os
import platform
import shutil
import sys
from pathlib import Path

from . import config as config_mod
from .catalog import catalog_rows, rows_json, rows_table, selected_status_hosts
from .checks import compose_validation
from .config import Manifest, ManifestError, Skill, load_manifest, selected_skills
from .data import skill_data_path
from .frontmatter import FrontmatterError, parse_frontmatter
from .hashing import hash_directory
from .hosts import all_hosts
from .installer import copy_install, link_install, uninstall
from .manifest_edit import ManifestEditError, set_skill_enabled
from .planner import Action, Status, plan_install, plan_uninstall, status_of
from .sharing import plan_share_from_host, share_plan_json, share_plan_table
from .state import State


def find_repo_root(start: Path | None = None) -> Path:
    """Walk upward from *start* to the directory containing ``my-skills.toml``."""
    start = (start or Path.cwd()).resolve()
    for directory in (start, *start.parents):
        if (directory / "my-skills.toml").is_file():
            return directory
    raise ManifestError(
        "my-skills.toml not found in the current directory or any parent"
    )


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


def _validate_selected(manifest: Manifest, skills: list[Skill]) -> bool:
    """Print validation failures; return True if every skill is valid."""
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


def _apply_plan(plan, state) -> tuple[int, bool]:
    """Execute CREATE/UPDATE per item mode, report the rest. Returns (changed, blocked)."""
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


def _install_plan_json(plan) -> dict:
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
        manifest = _load(args)
    except ManifestError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2

    try:
        skills, hosts = _select(args, manifest)
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


def cmd_share(args: argparse.Namespace) -> int:
    try:
        manifest = _load(args)
        plan = plan_share_from_host(manifest, args.from_host)
    except ManifestError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2

    if args.plan:
        if args.json:
            print(json.dumps(share_plan_json(plan), indent=2))
        else:
            print(share_plan_table(plan))
        return 0

    print("error: share currently requires --plan", file=sys.stderr)
    return 2


def _cmd_set_enabled(args: argparse.Namespace, enabled: bool) -> int:
    try:
        root = find_repo_root()
        load_manifest(root)
        set_skill_enabled(root / "my-skills.toml", args.skill, enabled)
    except (ManifestError, ManifestEditError) as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2
    verb = "enabled" if enabled else "disabled"
    print(f"{verb}: {args.skill}")
    return 0


def cmd_enable(args: argparse.Namespace) -> int:
    return _cmd_set_enabled(args, True)


def cmd_disable(args: argparse.Namespace) -> int:
    return _cmd_set_enabled(args, False)


# ------------------------------------------------------------------ status ---


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
            print(f"  {host:8} {status_of(manifest, skill, host, state).value}")
    return 0


# ------------------------------------------------------------------ skills ---


def cmd_skills(args: argparse.Namespace) -> int:
    try:
        manifest = _load(args)
    except ManifestError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2

    host = args.host
    enabled = True if args.enabled else None
    if args.disabled:
        enabled = False

    state = State.load() if args.with_status else None
    status_hosts = selected_status_hosts(manifest, host) if args.with_status else None

    try:
        rows = catalog_rows(
            manifest,
            host=host,
            enabled=enabled,
            status_hosts=status_hosts,
            status_lookup=(
                None
                if state is None
                else lambda skill, target: status_of(manifest, skill, target, state)
            ),
        )
    except ManifestError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2

    if args.json:
        print(json.dumps(rows_json(rows), indent=2))
    else:
        print(rows_table(rows, with_status=args.with_status))
    return 0


# -------------------------------------------------------------------- sync ---


def cmd_sync(args: argparse.Namespace) -> int:
    try:
        manifest = _load(args)
    except ManifestError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2

    try:
        skills, hosts = _select(args, manifest)
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


# ------------------------------------------------------------------ import ---


def cmd_import(args: argparse.Namespace) -> int:
    """Import an external skill directory into canonical ``skills/`` (plan 8.3)."""
    try:
        manifest = _load(args)
    except ManifestError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2

    source = Path(args.source).expanduser()
    if not (source / "SKILL.md").is_file():
        print(f"error: {source} is not an Agent Skill (no SKILL.md)", file=sys.stderr)
        return 2

    # Validate the source against the standard + security scan before trusting it.
    result = compose_validation(source)
    if not result.ok:
        print(f"[BLOCKED] {source.name}: validation failed")
        for error in result.errors:
            print(f"  error: {error}")
        print("\nNothing was imported (fix the source first).", file=sys.stderr)
        return 1

    try:
        meta, _ = parse_frontmatter((source / "SKILL.md").read_text(encoding="utf-8"))
    except FrontmatterError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1
    name = str(meta["name"])
    target_dir = manifest.skills_dir / name

    if target_dir.exists():
        if hash_directory(target_dir) == hash_directory(source):
            print(f"up to date: {name} already matches the canonical skill")
            return 0
        if not args.force:
            print(f"[BLOCKED] {name}: a different canonical skill already exists at")
            print(f"  {target_dir}")
            print("Re-run with --force to overwrite it. Nothing was changed.", file=sys.stderr)
            return 1
        shutil.rmtree(target_dir)

    target_dir.parent.mkdir(parents=True, exist_ok=True)
    shutil.copytree(source, target_dir)
    print(f"imported: {name} -> {target_dir}")
    print(f"Next: add [skills.{name}] to my-skills.toml, then `my-skills sync`.")
    return 0


# ---------------------------------------------------------------- data-path ---


def cmd_data_path(args: argparse.Namespace) -> int:
    """Print a skill's shared machine-local data directory (plan 15.4).

    Pure path resolver: it needs no manifest/repo root, so it runs anywhere.
    """
    try:
        path = skill_data_path(args.skill, create=args.create)
    except ValueError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2
    print(path)
    return 0


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
    p_install.add_argument("--json", action="store_true", help="Print dry-run plan as JSON")
    p_install.set_defaults(func=cmd_install)

    p_share = sub.add_parser("share", help="Plan sharing host-local skills into canonical")
    p_share.add_argument("skill", nargs="?", help="Host skill name")
    p_share.add_argument("--from", dest="from_host", required=True, help="Source host name")
    p_share.add_argument("--plan", action="store_true", help="Show candidate plan; change nothing")
    p_share.add_argument("--json", action="store_true", help="Print machine-readable JSON")
    p_share.set_defaults(func=cmd_share)

    p_enable = sub.add_parser("enable", help="Enable a registered skill in the manifest")
    p_enable.add_argument("skill", help="Skill name")
    p_enable.set_defaults(func=cmd_enable)

    p_disable = sub.add_parser("disable", help="Disable a registered skill in the manifest")
    p_disable.add_argument("skill", help="Skill name")
    p_disable.set_defaults(func=cmd_disable)

    p_status = sub.add_parser("status", help="Show install status per skill and host")
    p_status.set_defaults(func=cmd_status)

    p_skills = sub.add_parser("skills", help="List registered canonical skills")
    p_skills.add_argument("--json", action="store_true", help="Print machine-readable JSON")
    p_skills.add_argument("--host", help="Show skills compatible with a host")
    state_filter = p_skills.add_mutually_exclusive_group()
    state_filter.add_argument("--enabled", action="store_true", help="Only enabled skills")
    state_filter.add_argument("--disabled", action="store_true", help="Only disabled skills")
    p_skills.add_argument("--with-status", action="store_true", help="Include install status")
    p_skills.set_defaults(func=cmd_skills)

    p_sync = sub.add_parser("sync", help="Update managed installs from canonical (copy mode)")
    p_sync.add_argument("skill", nargs="?", help="Skill name (default: enabled skills)")
    p_sync.add_argument("--host", help="Target host name, or 'all' (default: enabled targets)")
    p_sync.add_argument("--all", action="store_true", help="Include every registered skill")
    p_sync.add_argument("--check", action="store_true", help="Report drift only; change nothing")
    p_sync.set_defaults(func=cmd_sync)

    p_uninstall = sub.add_parser("uninstall", help="Remove managed installs")
    p_uninstall.add_argument("skill", nargs="?", help="Skill name")
    p_uninstall.add_argument("--host", help="Target host name, or 'all' (default: enabled targets)")
    p_uninstall.set_defaults(func=cmd_uninstall)

    p_import = sub.add_parser(
        "import", help="Import an external skill directory into canonical skills/"
    )
    p_import.add_argument("source", help="Path to a skill directory (contains SKILL.md)")
    p_import.add_argument(
        "--force", action="store_true", help="Overwrite an existing different canonical skill"
    )
    p_import.set_defaults(func=cmd_import)

    p_data = sub.add_parser(
        "data-path",
        help="Print a skill's shared machine-local data directory",
    )
    p_data.add_argument("skill", help="Skill name")
    p_data.add_argument(
        "--create", action="store_true", help="Create the directory if missing"
    )
    p_data.set_defaults(func=cmd_data_path)

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
