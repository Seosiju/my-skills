from __future__ import annotations

import argparse
import os
from pathlib import Path

from .config import Manifest, ManifestError, Skill, load_manifest, selected_skills


def _root_cache_path() -> Path:
    """Machine-local cache recording the project root, for cwd-independent runs."""
    xdg = os.environ.get("XDG_CONFIG_HOME")
    base = Path(xdg) if xdg else Path.home() / ".config"
    return base / "my-skills" / "root"


def _is_root(path: Path) -> bool:
    return (path / "my-skills.toml").is_file()


def _write_root_cache(root: Path) -> None:
    """Record the discovered root; best-effort, never fail a command over it."""
    cache = _root_cache_path()
    try:
        cache.parent.mkdir(parents=True, exist_ok=True)
        cache.write_text(str(root) + "\n", encoding="utf-8")
    except OSError:
        pass


def find_repo_root(start: Path | None = None) -> Path:
    """Locate the project root (the directory holding ``my-skills.toml``).

    Resolution order, highest precedence first:

    1. ``$MY_SKILLS_ROOT`` — an explicit override; an invalid value is an error.
    2. ``my-skills.toml`` found in *start* (or cwd) or any parent. On success the
       location is cached so later runs work from any directory.
    3. The cached root written by a previous successful run.

    This lets the CLI — and the host-installed ``my-skills`` skill that shells out
    to it — run from anywhere once the root has been seen once.
    """
    env = os.environ.get("MY_SKILLS_ROOT")
    if env:
        root = Path(env).expanduser().resolve()
        if _is_root(root):
            return root
        raise ManifestError(
            f"MY_SKILLS_ROOT={env} does not contain my-skills.toml"
        )

    start = (start or Path.cwd()).resolve()
    for directory in (start, *start.parents):
        if (directory / "my-skills.toml").is_file():
            _write_root_cache(directory)
            return directory

    cache = _root_cache_path()
    if cache.is_file():
        try:
            cached = Path(cache.read_text(encoding="utf-8").strip()).expanduser()
        except OSError:
            cached = None
        if cached and _is_root(cached):
            return cached.resolve()

    raise ManifestError(
        "my-skills.toml not found. Run a my-skills command once from your "
        "clone of the repo, or set MY_SKILLS_ROOT to its path."
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
