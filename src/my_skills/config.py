"""Load and resolve the ``my-skills.toml`` manifest.

Precedence (highest first), per plan section 10::

    CLI option  >  my-skills.local.toml  >  my-skills.toml  >  built-in default
"""

from __future__ import annotations

import os
import tomllib
from dataclasses import dataclass, field
from pathlib import Path

# Built-in default user-scope install paths (plan section 5.4 / decision 3).
BUILTIN_TARGET_PATHS: dict[str, str] = {
    "claude": "~/.claude/skills",
    "codex": "~/.agents/skills",
    "gemini": "~/.gemini/skills",
    "hermes": "~/.hermes/skills",
}


class ManifestError(ValueError):
    """Raised when a manifest is missing or structurally invalid."""


def expand_path(value: str) -> Path:
    """Expand ``~`` and environment variables; return an absolute path.

    Symlinks are not resolved so the path reflects the user's configured
    location, but the result is always absolute.
    """
    p = Path(os.path.expandvars(value)).expanduser()
    if not p.is_absolute():
        p = Path.cwd() / p
    return p


@dataclass
class Defaults:
    install_mode: str = "copy"
    collision: str = "error"
    verify_after_install: bool = True


@dataclass
class Target:
    name: str
    enabled: bool
    scope: str
    path: Path


@dataclass
class Skill:
    name: str
    enabled: bool
    hosts: list[str] = field(default_factory=list)


@dataclass
class Manifest:
    schema_version: int
    skills_root: str
    defaults: Defaults
    targets: dict[str, Target]
    skills: dict[str, Skill]
    root: Path

    @property
    def skills_dir(self) -> Path:
        return self.root / self.skills_root


def _read_toml(path: Path) -> dict:
    try:
        with path.open("rb") as fh:
            return tomllib.load(fh)
    except tomllib.TOMLDecodeError as exc:
        raise ManifestError(f"invalid TOML in {path}: {exc}") from exc


def _resolve_defaults(data: dict, local: dict) -> Defaults:
    merged = {**data.get("defaults", {}), **local.get("defaults", {})}
    return Defaults(
        install_mode=merged.get("install_mode", "copy"),
        collision=merged.get("collision", "error"),
        verify_after_install=merged.get("verify_after_install", True),
    )


def _resolve_targets(data: dict, local: dict, cli: dict) -> dict[str, Target]:
    cli_targets = cli.get("targets", {})
    names = (
        set(BUILTIN_TARGET_PATHS)
        | set(data.get("targets", {}))
        | set(local.get("targets", {}))
        | set(cli_targets)
    )
    result: dict[str, Target] = {}
    for name in sorted(names):
        merged = {
            **data.get("targets", {}).get(name, {}),
            **local.get("targets", {}).get(name, {}),
            **cli_targets.get(name, {}),
        }
        path_value = merged.get("path") or BUILTIN_TARGET_PATHS.get(name)
        if not path_value:
            raise ManifestError(
                f"target '{name}' has no path and no built-in default"
            )
        result[name] = Target(
            name=name,
            enabled=bool(merged.get("enabled", True)),
            scope=str(merged.get("scope", "user")),
            path=expand_path(path_value),
        )
    return result


def _resolve_skills(data: dict, local: dict) -> dict[str, Skill]:
    names = set(data.get("skills", {})) | set(local.get("skills", {}))
    result: dict[str, Skill] = {}
    for name in sorted(names):
        merged = {
            **data.get("skills", {}).get(name, {}),
            **local.get("skills", {}).get(name, {}),
        }
        result[name] = Skill(
            name=name,
            enabled=bool(merged.get("enabled", True)),
            hosts=list(merged.get("hosts", [])),
        )
    return result


def load_manifest(
    root: Path | str,
    *,
    cli_overrides: dict | None = None,
) -> Manifest:
    """Load ``my-skills.toml`` (and optional ``my-skills.local.toml``) from *root*."""
    root = Path(root)
    main_path = root / "my-skills.toml"
    if not main_path.is_file():
        raise ManifestError(f"manifest not found: {main_path}")

    data = _read_toml(main_path)
    local_path = root / "my-skills.local.toml"
    local = _read_toml(local_path) if local_path.is_file() else {}
    cli = cli_overrides or {}

    skills_root = (
        cli.get("skills_root")
        or local.get("skills_root")
        or data.get("skills_root", "skills")
    )

    return Manifest(
        schema_version=int(data.get("schema_version", 1)),
        skills_root=skills_root,
        defaults=_resolve_defaults(data, local),
        targets=_resolve_targets(data, local, cli),
        skills=_resolve_skills(data, local),
        root=root,
    )


def selected_skills(
    manifest: Manifest,
    *,
    explicit: list[str] | None = None,
    all: bool = False,
) -> list[Skill]:
    """Resolve which skills a command targets.

    Per plan sections 5.5 / 9.5 / 9.6: with no explicit skill and without
    ``--all``, only skills with ``enabled = true`` are selected. ``--all``
    selects every registered skill. An explicit list selects exactly those
    skills (each must exist).
    """
    if explicit:
        missing = [n for n in explicit if n not in manifest.skills]
        if missing:
            raise ManifestError(f"unknown skill(s): {', '.join(sorted(missing))}")
        return [manifest.skills[n] for n in explicit]
    if all:
        return list(manifest.skills.values())
    return [s for s in manifest.skills.values() if s.enabled]
