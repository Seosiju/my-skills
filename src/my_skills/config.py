"""Load and resolve the ``my-skills.toml`` manifest.

Precedence (highest first), per plan section 10::

    CLI option  >  my-skills.local.toml  >  my-skills.toml  >  built-in default
"""

from __future__ import annotations

import os
import tomllib
from collections.abc import Iterable
from dataclasses import dataclass, field
from pathlib import Path
from typing import cast

# Built-in default user-scope install paths (plan section 5.4 / decision 3).
BUILTIN_TARGET_PATHS: dict[str, str] = {
    "claude": "~/.claude/skills",
    "codex": "~/.agents/skills",
    "hermes": "~/.hermes/skills",
}
TomlTable = dict[str, object]


def _as_table(value: object) -> TomlTable:
    if not isinstance(value, dict):
        return {}
    raw_table = cast(dict[object, object], value)
    table: TomlTable = {}
    for key, item in raw_table.items():
        if isinstance(key, str):
            table[key] = item
    return table


def _as_str(value: object, default: str = "") -> str:
    return value if isinstance(value, str) else default


def _as_bool(value: object, default: bool) -> bool:
    return value if isinstance(value, bool) else default


def _as_int(value: object, default: int) -> int:
    if isinstance(value, int):
        return value
    if isinstance(value, str):
        try:
            return int(value)
        except ValueError:
            return default
    return default


def _as_str_list(value: object) -> list[str]:
    if not isinstance(value, Iterable) or isinstance(value, str):
        return []
    return [item for item in value if isinstance(item, str)]


def _merge_tables(*tables: TomlTable) -> TomlTable:
    merged: TomlTable = {}
    for table in tables:
        merged.update(table)
    return merged


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
class AuditSettings:
    enabled: bool = True
    profile: str = "default"
    threshold: str | None = None
    disabled_rules: list[str] = field(default_factory=list)


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
    source_type: str = ""
    source_revision: str = ""


@dataclass
class Manifest:
    schema_version: int
    skills_root: str
    defaults: Defaults
    targets: dict[str, Target]
    skills: dict[str, Skill]
    root: Path
    audit: AuditSettings = field(default_factory=AuditSettings)

    @property
    def skills_dir(self) -> Path:
        return self.root / self.skills_root


def _read_toml(path: Path) -> TomlTable:
    try:
        with path.open("rb") as fh:
            return _as_table(tomllib.load(fh))
    except tomllib.TOMLDecodeError as exc:
        raise ManifestError(f"invalid TOML in {path}: {exc}") from exc


def _resolve_defaults(data: TomlTable, local: TomlTable) -> Defaults:
    merged = _merge_tables(
        _as_table(data.get("defaults")),
        _as_table(local.get("defaults")),
    )
    return Defaults(
        install_mode=_as_str(merged.get("install_mode"), "copy"),
        collision=_as_str(merged.get("collision"), "error"),
        verify_after_install=_as_bool(merged.get("verify_after_install"), True),
    )


def _resolve_audit(data: TomlTable, local: TomlTable) -> AuditSettings:
    merged = _merge_tables(
        _as_table(data.get("audit")),
        _as_table(local.get("audit")),
    )
    return AuditSettings(
        enabled=_as_bool(merged.get("enabled"), True),
        profile=_as_str(merged.get("profile"), "default"),
        threshold=_as_str(merged.get("threshold")) or None,
        disabled_rules=_as_str_list(merged.get("disabled_rules")),
    )


def _resolve_targets(data: TomlTable, local: TomlTable, cli: TomlTable) -> dict[str, Target]:
    data_targets = _as_table(data.get("targets"))
    local_targets = _as_table(local.get("targets"))
    cli_targets = _as_table(cli.get("targets"))
    names = (
        set(BUILTIN_TARGET_PATHS)
        | set(data_targets)
        | set(local_targets)
        | set(cli_targets)
    )
    result: dict[str, Target] = {}
    for name in sorted(names):
        merged = _merge_tables(
            _as_table(data_targets.get(name)),
            _as_table(local_targets.get(name)),
            _as_table(cli_targets.get(name)),
        )
        path_value = _as_str(merged.get("path")) or BUILTIN_TARGET_PATHS.get(name)
        if not path_value:
            raise ManifestError(
                f"target '{name}' has no path and no built-in default"
            )
        result[name] = Target(
            name=name,
            enabled=_as_bool(merged.get("enabled"), True),
            scope=_as_str(merged.get("scope"), "user"),
            path=expand_path(path_value),
        )
    return result


def _resolve_skills(data: TomlTable, local: TomlTable) -> dict[str, Skill]:
    data_skills = _as_table(data.get("skills"))
    local_skills = _as_table(local.get("skills"))
    names = set(data_skills) | set(local_skills)
    result: dict[str, Skill] = {}
    for name in sorted(names):
        merged = _merge_tables(
            _as_table(data_skills.get(name)),
            _as_table(local_skills.get(name)),
        )
        result[name] = Skill(
            name=name,
            enabled=_as_bool(merged.get("enabled"), True),
            hosts=_as_str_list(merged.get("hosts")),
            source_type=_as_str(merged.get("source_type")),
            source_revision=_as_str(merged.get("source_revision")),
        )
    return result


def load_manifest(
    root: Path | str,
    *,
    cli_overrides: TomlTable | None = None,
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
        _as_str(cli.get("skills_root"))
        or _as_str(local.get("skills_root"))
        or _as_str(data.get("skills_root"), "skills")
    )

    return Manifest(
        schema_version=_as_int(data.get("schema_version"), 1),
        skills_root=skills_root,
        defaults=_resolve_defaults(data, local),
        audit=_resolve_audit(data, local),
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
