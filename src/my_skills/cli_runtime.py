from __future__ import annotations

import argparse
import os
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Literal

from .config import Manifest, ManifestError, Skill, load_manifest, selected_skills

RootSource = Literal["env", "cwd", "cache"]


@dataclass(frozen=True, slots=True)
class RootResolution:
    root: Path
    source: RootSource
    cached: Path | None = None


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


def _read_root_cache() -> Path | None:
    cache = _root_cache_path()
    if not cache.is_file():
        return None
    try:
        cached = Path(cache.read_text(encoding="utf-8").strip()).expanduser()
    except OSError:
        return None
    if _is_root(cached):
        return cached.resolve()
    return None


def cache_repo_root(root: Path) -> None:
    resolved = root.expanduser().resolve()
    if not _is_root(resolved):
        raise ManifestError(f"{root} does not contain my-skills.toml")
    _write_root_cache(resolved)


def find_cwd_root(start: Path | None = None) -> Path:
    start = (start or Path.cwd()).resolve()
    for directory in (start, *start.parents):
        if _is_root(directory):
            return directory
    raise ManifestError(
        "my-skills.toml not found. Run this command from a registry, pass a "
        "registry path, or run 'my-skills init-registry' first."
    )


def resolve_root(start: Path | None = None, *, write_cache: bool = True) -> RootResolution:
    env = os.environ.get("MY_SKILLS_ROOT")
    if env:
        root = Path(env).expanduser().resolve()
        if _is_root(root):
            return RootResolution(root=root, source="env", cached=_read_root_cache())
        raise ManifestError(
            f"MY_SKILLS_ROOT={env} does not contain my-skills.toml"
        )

    cached = _read_root_cache()
    try:
        cwd_root = find_cwd_root(start)
    except ManifestError:
        cwd_root = None
    if cwd_root is not None:
        if write_cache:
            if cached is None:
                _write_root_cache(cwd_root)
                cached = cwd_root
            elif cached != cwd_root.resolve():
                print(
                    "note: using ./my-skills.toml for this command; "
                    f"active registry is {cached}. Run 'my-skills set-root' "
                    "here to switch.",
                    file=sys.stderr,
                )
        return RootResolution(root=cwd_root, source="cwd", cached=cached)

    if cached is not None:
        return RootResolution(root=cached, source="cache", cached=cached)

    raise ManifestError(
        "my-skills.toml not found. Run 'my-skills init-registry' to create a "
        "registry, run 'my-skills set-root <path>' to select one, or set "
        "MY_SKILLS_ROOT to its path."
    )


def find_repo_root(start: Path | None = None, *, write_cache: bool = True) -> Path:
    """Locate the project root (the directory holding ``my-skills.toml``).

    Resolution order, highest precedence first:

    1. ``$MY_SKILLS_ROOT`` — an explicit override; an invalid value is an error.
    2. ``my-skills.toml`` found in *start* (or cwd) or any parent. The location
       is cached only when no valid active root already exists.
    3. The cached root written by a previous successful run.

    This lets the CLI — and the host-installed ``my-skills`` skill that shells out
    to it — run from anywhere once the root has been seen once.
    """
    return resolve_root(start, write_cache=write_cache).root


def cmd_set_root(args: argparse.Namespace) -> int:
    path = getattr(args, "path", None)
    try:
        root = Path(path).expanduser().resolve() if path else find_cwd_root().resolve()
        cache_repo_root(root)
    except ManifestError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2
    print(f"Active registry root set to {root}")
    return 0


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
