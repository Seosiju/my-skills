from __future__ import annotations

import argparse
import sys

from .config import BUILTIN_TARGET_PATHS, Manifest, Skill, expand_path


def confirm_multi_host_write(
    args: argparse.Namespace,
    hosts: list[str],
    *,
    allow_without_yes: bool = False,
) -> bool:
    if len(hosts) <= 1 or getattr(args, "yes", False) or allow_without_yes:
        return True
    host_list = ", ".join(hosts)
    print(
        f"error: this command writes to multiple hosts ({host_list}). "
        "Re-run with --yes after reviewing a dry-run plan.",
        file=sys.stderr,
    )
    return False


def is_builtin_seed_default_install(
    args: argparse.Namespace,
    manifest: Manifest,
    skills: list[Skill],
    hosts: list[str],
) -> bool:
    if (
        getattr(args, "skill", None)
        or getattr(args, "host", None)
        or getattr(args, "all", False)
        or getattr(args, "mode", "copy") != "copy"
        or getattr(args, "skip_audit", False)
        or not skills
    ):
        return False
    if any(skill.source_type != "builtin-seed" for skill in skills):
        return False
    return all(
        host in BUILTIN_TARGET_PATHS
        and manifest.targets[host].path == expand_path(BUILTIN_TARGET_PATHS[host])
        for host in hosts
    )
