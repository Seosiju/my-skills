from __future__ import annotations

import json
import re
from pathlib import Path
from typing import Iterable

from .validation import validate_name


class ManifestEditError(ValueError):
    pass


_SECTION_RE = re.compile(r"^\[([^\]]+)\]\s*$")
_ENABLED_RE = re.compile(r"^\s*enabled\s*=")
_HOSTS_RE = re.compile(r"^\s*hosts\s*=")


def set_skill_enabled(manifest_path: Path, skill: str, enabled: bool) -> None:
    lines = _read_lines(manifest_path)
    start, end = _skill_section(lines, skill)
    if start is None:
        raise ManifestEditError(f"unknown skill: {skill}")
    _set_key(lines, start + 1, end, _ENABLED_RE, f"enabled = {_toml_bool(enabled)}")
    _write_lines(manifest_path, lines)


def register_skill(
    manifest_path: Path,
    skill: str,
    *,
    enabled: bool,
    hosts: Iterable[str],
) -> None:
    _validate_skill_name(skill)
    lines = _read_lines(manifest_path)
    start, end = _skill_section(lines, skill)
    if start is None:
        if lines and lines[-1] != "":
            lines.append("")
        lines.extend(
            [
                f"[skills.{skill}]",
                f"enabled = {_toml_bool(enabled)}",
                f"hosts = {_toml_array(hosts)}",
            ]
        )
    else:
        _set_key(lines, start + 1, end, _ENABLED_RE, f"enabled = {_toml_bool(enabled)}")
        _set_key(lines, start + 1, end, _HOSTS_RE, f"hosts = {_toml_array(hosts)}")
    _write_lines(manifest_path, lines)


def _validate_skill_name(skill: str) -> None:
    errors = validate_name(skill)
    if errors:
        raise ManifestEditError(f"invalid skill name '{skill}': {errors[0]}")


def _read_lines(manifest_path: Path) -> list[str]:
    return Path(manifest_path).read_text(encoding="utf-8").splitlines()


def _write_lines(manifest_path: Path, lines: list[str]) -> None:
    Path(manifest_path).write_text("\n".join(lines).rstrip() + "\n", encoding="utf-8")


def _skill_section(lines: list[str], skill: str) -> tuple[int | None, int]:
    target = f"skills.{skill}"
    start: int | None = None
    for index, line in enumerate(lines):
        match = _SECTION_RE.match(line.strip())
        if not match:
            continue
        if start is not None:
            return start, index
        if match.group(1) == target:
            start = index
    return start, len(lines)


def _set_key(
    lines: list[str],
    start: int,
    end: int,
    pattern: re.Pattern[str],
    replacement: str,
) -> None:
    for index in range(start, end):
        if pattern.match(lines[index]):
            lines[index] = replacement
            return
    lines.insert(end, replacement)


def _toml_bool(value: bool) -> str:
    return "true" if value else "false"


def _toml_array(values: Iterable[str]) -> str:
    return json.dumps(list(values))
