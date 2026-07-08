from __future__ import annotations

import argparse
import json
import shutil
import sys
from pathlib import Path

from .audit.formatting import format_gate
from .audit.gate import audit_policy_from_manifest, audit_skills
from .checks import compose_validation
from .cli_runtime import find_repo_root
from .config import Manifest, ManifestError, load_manifest
from .data import skill_data_path
from .frontmatter import FrontmatterError, parse_frontmatter
from .hashing import hash_directory
from .manifest_edit import (
    ManifestEditError,
    has_skill_section,
    register_skill,
    set_skill_enabled,
)
from .sharing import (
    ShareBlockedError,
    apply_share_from_host,
    plan_share_from_host,
    share_plan_json,
    share_plan_table,
)
from .state import State, StateError


def cmd_share(args: argparse.Namespace) -> int:
    try:
        root = find_repo_root()
        manifest = load_manifest(root)
    except ManifestError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2

    if args.plan:
        try:
            plan = plan_share_from_host(manifest, args.from_host)
        except ManifestError as exc:
            print(f"error: {exc}", file=sys.stderr)
            return 2
        if args.json:
            print(json.dumps(share_plan_json(plan), indent=2))
        else:
            print(share_plan_table(plan))
        return 0

    if not args.skill:
        print("error: share requires a skill name unless --plan is used", file=sys.stderr)
        return 2
    if args.enable == args.disable:
        print("error: share requires exactly one of --enable or --disable", file=sys.stderr)
        return 2

    try:
        state = State.load()
        result = apply_share_from_host(
            manifest,
            root / "my-skills.toml",
            args.from_host,
            args.skill,
            enabled=args.enable,
            force=args.force,
            state=state,
            skip_audit=args.skip_audit,
        )
    except ShareBlockedError as exc:
        print(f"[BLOCKED] {exc}")
        return 1
    except StateError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2
    except (ManifestError, ManifestEditError) as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2

    state = "enabled" if result.enabled else "disabled"
    if args.force:
        print("WARN: force overwrite allowed for existing canonical skill")
    if args.skip_audit:
        print("WARN: audit skipped by explicit --skip-audit")
    print(f"shared: {result.name} -> {result.canonical} ({state})")
    print(f"adopted: {result.name} -> {result.adopted_host}")
    return 0


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


def _register_imported_skill(
    manifest: Manifest,
    manifest_path: Path,
    name: str,
    *,
    enabled: bool,
) -> str:
    if not has_skill_section(manifest_path, name):
        hosts = tuple(
            target_name
            for target_name, target in manifest.targets.items()
            if target.enabled
        )
        register_skill(manifest_path, name, enabled=enabled, hosts=hosts)
        return "enabled" if enabled else "disabled"
    if enabled:
        set_skill_enabled(manifest_path, name, True)
        return "enabled"
    return "enabled" if manifest.skills[name].enabled else "disabled"


def _first_symlink(root: Path) -> Path | None:
    if root.is_symlink():
        return root
    for path in root.rglob("*"):
        if path.is_symlink():
            return path
    return None


def cmd_import(args: argparse.Namespace) -> int:
    try:
        root = find_repo_root()
        manifest = load_manifest(root)
    except ManifestError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2

    source = Path(args.source).expanduser()
    if not (source / "SKILL.md").is_file():
        print(f"error: {source} is not an Agent Skill (no SKILL.md)", file=sys.stderr)
        return 2

    symlink = _first_symlink(source)
    if symlink is not None:
        relative_symlink = (
            Path(".") if symlink == source else symlink.relative_to(source)
        )
        print(f"[BLOCKED] {source.name}: import source contains symlink")
        print(f"  symlink: {relative_symlink}")
        print("\nNothing was imported (fix the source first).", file=sys.stderr)
        return 1

    result = compose_validation(source)
    if not result.ok:
        print(f"[BLOCKED] {source.name}: validation failed")
        for error in result.errors:
            print(f"  error: {error}")
        print("\nNothing was imported (fix the source first).", file=sys.stderr)
        return 1

    gate = audit_skills(
        (source,),
        policy=audit_policy_from_manifest(manifest),
        skip=args.skip_audit,
    )
    if gate.skipped:
        print(format_gate(gate))
    elif gate.blocked:
        print(format_gate(gate))
        print("\nNothing was imported (audit blocked).", file=sys.stderr)
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
            try:
                state = _register_imported_skill(
                    manifest,
                    root / "my-skills.toml",
                    name,
                    enabled=args.enable,
                )
            except ManifestEditError as exc:
                print(f"error: {exc}", file=sys.stderr)
                return 2
            print(f"registered: {name} ({state})")
            return 0
        if not args.force:
            print(f"[BLOCKED] {name}: a different canonical skill already exists at")
            print(f"  {target_dir}")
            print("Re-run with --force to overwrite it. Nothing was changed.", file=sys.stderr)
            return 1
        print(f"WARN: force overwrite allowed for existing canonical skill: {target_dir}")
        shutil.rmtree(target_dir)

    target_dir.parent.mkdir(parents=True, exist_ok=True)
    shutil.copytree(source, target_dir)
    print(f"imported: {name} -> {target_dir}")
    try:
        state = _register_imported_skill(
            manifest,
            root / "my-skills.toml",
            name,
            enabled=args.enable,
        )
    except ManifestEditError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2
    print(f"registered: {name} ({state})")
    if args.enable:
        print(f"Next: `my-skills install {name} --host <host>`.")
    else:
        print(f"Next: `my-skills enable {name}`, then `my-skills install {name} --host <host>`.")
    return 0


def cmd_data_path(args: argparse.Namespace) -> int:
    try:
        path = skill_data_path(args.skill, create=args.create)
    except ValueError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2
    print(path)
    return 0
