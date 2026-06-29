from __future__ import annotations

import argparse
from pathlib import Path

from .config import Manifest, ManifestError, Skill, load_manifest, selected_skills


def find_repo_root(start: Path | None = None) -> Path:
    start = (start or Path.cwd()).resolve()
    for directory in (start, *start.parents):
        if (directory / "my-skills.toml").is_file():
            return directory
    raise ManifestError(
        "my-skills.toml not found in the current directory or any parent"
    )


def load_manifest_from_cwd() -> Manifest:
    return load_manifest(find_repo_root())


def resolve_hosts(manifest: Manifest, host_arg: str | None) -> list[str]:
    if host_arg and host_arg != "all":
        if host_arg not in manifest.targets:
            raise ManifestError(f"unknown host: {host_arg}")
        return [host_arg]
    return [name for name, target in manifest.targets.items() if target.enabled]


def select_requested(
    args: argparse.Namespace,
    manifest: Manifest,
) -> tuple[list[Skill], list[str]]:
    skills = selected_skills(
        manifest,
        explicit=[args.skill] if getattr(args, "skill", None) else None,
        all=getattr(args, "all", False),
    )
    hosts = resolve_hosts(manifest, getattr(args, "host", None))
    return skills, hosts
