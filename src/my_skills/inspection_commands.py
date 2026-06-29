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
from .cli_runtime import load_manifest_from_cwd
from .config import Manifest, ManifestError
from .hosts import all_hosts
from .planner import status_of
from .state import State


def _skill_dirs(manifest: Manifest, skill: str | None) -> list[Path]:
    skills_dir = manifest.skills_dir
    if skill:
        return [skills_dir / skill]
    if not skills_dir.is_dir():
        return []
    return sorted(path for path in skills_dir.iterdir() if path.is_dir())


def cmd_validate(args: argparse.Namespace) -> int:
    try:
        manifest = load_manifest_from_cwd()
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
        manifest = load_manifest_from_cwd()
    except ManifestError as exc:
        manifest_error = exc

    print("\nHosts:")
    for host in all_hosts():
        exe = next((command for command in host.detect_commands if shutil.which(command)), None)
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


def cmd_status(args: argparse.Namespace) -> int:
    try:
        manifest = load_manifest_from_cwd()
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


def cmd_skills(args: argparse.Namespace) -> int:
    try:
        manifest = load_manifest_from_cwd()
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
