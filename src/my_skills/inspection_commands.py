from __future__ import annotations

import argparse
import json
import os
import platform
import shutil
import sys
from pathlib import Path
from typing import Callable

from .audit.analyzers import run_audit
from .audit.gate import audit_policy_from_manifest
from . import __version__
from . import config as config_mod
from .catalog import catalog_rows, rows_json, rows_table, selected_status_hosts
from .checks import compose_validation
from .cli_runtime import resolve_root, load_manifest_from_cwd
from .config import Manifest, ManifestError, Skill
from .data import data_root
from .hosts import all_hosts
from .planner import status_of
from .state import State, StateError, default_state_path


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
    print(f"my-skills {__version__}")
    print(f"OS:     {platform.platform()}")
    print(f"Shell:  {os.environ.get('SHELL', 'unknown')}")
    print(f"Python: {platform.python_version()}")

    manifest = None
    manifest_error = None
    resolution = None
    try:
        resolution = resolve_root(write_cache=False)
        manifest = config_mod.load_manifest(resolution.root)
    except ManifestError as exc:
        manifest_error = exc

    print()
    if resolution is None:
        if manifest_error and not str(manifest_error).startswith("my-skills.toml not found"):
            print(f"Registry: INVALID ({manifest_error})")
        else:
            print(
                "Registry: not configured (run 'my-skills init-registry' or "
                "'my-skills set-root')"
            )
    else:
        print(f"Registry: {resolution.root} (source: {resolution.source})")
        if (
            resolution.source == "cwd"
            and resolution.cached is not None
            and resolution.cached != resolution.root
        ):
            print(
                "warning: this directory is not the active registry "
                f"(active: {resolution.cached})"
            )
    if manifest is not None:
        enabled = sum(1 for skill in manifest.skills.values() if skill.enabled)
        print(f"Skills:   {len(manifest.skills)} registered, {enabled} enabled")
    print(f"State:    {default_state_path()}")
    print(f"Data:     {data_root()}")

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
    if resolution is None:
        if str(manifest_error).startswith("my-skills.toml not found"):
            print("\nManifest: not checked")
            return 0
        print(f"\nManifest: INVALID ({manifest_error})")
        return 1
    print(f"\nManifest: INVALID ({manifest_error})")
    return 1


def cmd_status(args: argparse.Namespace) -> int:
    try:
        manifest = load_manifest_from_cwd()
    except ManifestError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2

    try:
        state = State.load()
    except StateError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2
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

    try:
        state = State.load()
    except StateError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2
    status_hosts = selected_status_hosts(manifest, host)
    governance_lookup: Callable[[Skill], tuple[dict[str, str], dict[str, str]]] | None = None
    if args.with_status:
        governance_lookup = lambda skill: _governance_for_skill(manifest, skill.name, state)

    try:
        rows = catalog_rows(
            manifest,
            host=host,
            enabled=enabled,
            status_hosts=status_hosts,
            status_lookup=lambda skill, target: status_of(manifest, skill, target, state),
            governance_lookup=governance_lookup,
        )
    except ManifestError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2

    if args.json:
        print(json.dumps(rows_json(rows), indent=2))
    else:
        print(rows_table(rows, status_hosts))
    return 0


def _governance_for_skill(
    manifest: Manifest,
    skill_name: str,
    state: State,
) -> tuple[dict[str, str], dict[str, str]]:
    result = run_audit(manifest.skills_dir / skill_name, policy=audit_policy_from_manifest(manifest))
    records = [
        record
        for record in state.records()
        if record.skill == skill_name and record.last_audit_result_hash
    ]
    record = next((item for item in records if item.source_type == "host"), None)
    record = record or (records[0] if records else None)
    last_hash = record.last_audit_result_hash if record else result.result_hash
    source_type = record.source_type if record else "canonical"
    trust_tier = "imported-local" if source_type == "host" else "local-authored"
    status = "blocked" if result.blocked else "ok"
    threshold = result.threshold.label if result.threshold else "none"
    return (
        {
            "status": status,
            "profile": result.profile,
            "threshold": threshold,
            "result_hash": result.result_hash,
            "findings": str(len(result.findings)),
        },
        {
            "source_type": source_type,
            "trust_tier": trust_tier,
            "last_audit_result_hash": last_hash,
        },
    )
