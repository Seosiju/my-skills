"""Agent Skills standard validation (plan sections 5.1, 9.3).

Structural validation only. The security scan lives in :mod:`my_skills.security`
and is composed with this at the CLI layer so each concern is tested in
isolation.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from pathlib import Path

from .frontmatter import FrontmatterError, parse_frontmatter
from .hosts.base import HostConfig

NAME_RE = re.compile(r"^[a-z0-9]+(?:-[a-z0-9]+)*$")
MAX_NAME_LEN = 64
MAX_DESCRIPTION_LEN = 1024

# Markdown inline link: ](target)
_LINK_RE = re.compile(r"\]\(([^)]+)\)")
_ABS_PATH_RE = re.compile(r"/Users/[^/\s]+|/home/[^/\s]+")


@dataclass
class ValidationResult:
    skill: str
    path: Path
    errors: list[str] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)

    @property
    def ok(self) -> bool:
        return not self.errors


def validate_name(name: str) -> list[str]:
    """Return error strings for an invalid skill name (empty list if valid)."""
    errors: list[str] = []
    if len(name) > MAX_NAME_LEN:
        errors.append(f"name exceeds {MAX_NAME_LEN} characters")
    if not NAME_RE.match(name):
        errors.append(
            "name must be lowercase alphanumeric words separated by single "
            "hyphens (no leading, trailing, or consecutive hyphens)"
        )
    return errors


def validate_skill(path: Path) -> ValidationResult:
    """Validate one canonical skill directory against the Agent Skills standard."""
    path = Path(path)
    result = ValidationResult(skill=path.name, path=path)

    skill_md = path / "SKILL.md"
    if not skill_md.is_file():
        result.errors.append("SKILL.md not found")
        return result

    text = skill_md.read_text(encoding="utf-8")
    try:
        meta, body = parse_frontmatter(text)
    except FrontmatterError as exc:
        result.errors.append(str(exc))
        return result

    name = meta.get("name")
    description = meta.get("description")

    if not name:
        result.errors.append("frontmatter is missing required field 'name'")
    else:
        result.errors.extend(validate_name(str(name)))
        if str(name) != path.name:
            result.errors.append(
                f"directory name '{path.name}' does not match frontmatter "
                f"name '{name}'"
            )

    if not description:
        result.errors.append("frontmatter is missing required field 'description'")
    elif len(str(description)) > MAX_DESCRIPTION_LEN:
        result.errors.append(
            f"description exceeds {MAX_DESCRIPTION_LEN} characters "
            f"({len(str(description))})"
        )

    # 'allowed-tools' has inconsistent host support (plan 5.1) -> warn.
    if "allowed-tools" in meta or "allowed_tools" in meta:
        result.warnings.append(
            "'allowed-tools' has inconsistent host support and may be ignored"
        )

    # Supporting-file references must exist.
    for ref in _LINK_RE.findall(body):
        ref = ref.strip()
        if ref.startswith(("http://", "https://", "#", "mailto:")):
            continue
        clean = ref.split("#", 1)[0].split("?", 1)[0].strip()
        if not clean or clean.startswith("/"):
            continue
        if not (path / clean).exists():
            result.errors.append(f"referenced supporting file not found: {ref}")

    # Absolute host path leakage in the body -> warning (prefer host-neutral).
    if _ABS_PATH_RE.search(body):
        result.errors.append(
            "body contains an absolute host path; prefer host-neutral relative paths"
        )

    return result


def validate_skill_for_host(path: Path, host: HostConfig) -> ValidationResult:
    result = validate_skill(path)
    if not result.ok:
        return result

    skill_md = path / "SKILL.md"
    meta, _body = parse_frontmatter(skill_md.read_text(encoding="utf-8"))
    description = str(meta.get("description", ""))
    if len(description) > host.description_max_chars:
        result.errors.append(
            f"{host.name} description exceeds {host.description_max_chars} characters"
        )
    extra = sorted(set(meta) - set(host.frontmatter_keep_fields) - set(host.optional_metadata))
    if extra:
        result.warnings.append(
            f"{host.name} may ignore unsupported frontmatter fields: {', '.join(extra)}"
        )
    return result
