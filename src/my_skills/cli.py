"""Command-line interface for my-skills (Phase 1: ``validate`` and ``doctor``)."""

from __future__ import annotations

import argparse
import os
import platform
import shutil
import sys
from pathlib import Path

from . import config as config_mod
from .config import ManifestError, load_manifest
from .hosts import all_hosts
from .security import scan_skill
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


def _skill_dirs(manifest, skill: str | None) -> list[Path]:
    skills_dir = manifest.skills_dir
    if skill:
        return [skills_dir / skill]
    if not skills_dir.is_dir():
        return []
    return sorted(p for p in skills_dir.iterdir() if p.is_dir())


def cmd_validate(args: argparse.Namespace) -> int:
    try:
        manifest = load_manifest(find_repo_root())
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
        manifest = load_manifest(find_repo_root())
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


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="my-skills",
        description="Personal cross-agent Agent Skill registry.",
    )
    sub = parser.add_subparsers(dest="command")

    p_validate = sub.add_parser(
        "validate",
        help="Validate canonical skills against the Agent Skills standard",
    )
    p_validate.add_argument(
        "skill", nargs="?", help="Skill name to validate (default: all skills)"
    )
    p_validate.set_defaults(func=cmd_validate)

    p_doctor = sub.add_parser(
        "doctor", help="Report environment, hosts, and manifest health"
    )
    p_doctor.set_defaults(func=cmd_doctor)

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
