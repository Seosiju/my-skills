from __future__ import annotations

from collections.abc import Callable
from dataclasses import dataclass
from pathlib import Path
from typing import NotRequired, TypedDict

from .config import Manifest, ManifestError, Skill
from .frontmatter import FrontmatterError, parse_frontmatter
from .planner import Status


class SkillJsonRow(TypedDict):
    name: str
    enabled: bool
    hosts: list[str]
    description: str
    path: str
    status: NotRequired[dict[str, str]]
    audit: NotRequired[dict[str, str]]
    provenance: NotRequired[dict[str, str]]
    error: NotRequired[str]


class CatalogJson(TypedDict):
    skills: list[SkillJsonRow]


@dataclass(frozen=True, slots=True)
class CatalogRow:
    name: str
    enabled: bool
    hosts: tuple[str, ...]
    description: str
    path: Path
    status: dict[str, Status] | None = None
    audit: dict[str, str] | None = None
    provenance: dict[str, str] | None = None
    error: str | None = None


StatusLookup = Callable[[Skill, str], Status]
GovernanceLookup = Callable[[Skill], tuple[dict[str, str], dict[str, str]]]


def selected_status_hosts(manifest: Manifest, host: str | None) -> list[str]:
    if host is not None:
        return [host]
    return [name for name, target in manifest.targets.items() if target.enabled]


def catalog_rows(
    manifest: Manifest,
    *,
    host: str | None = None,
    enabled: bool | None = None,
    status_hosts: list[str] | None = None,
    status_lookup: StatusLookup | None = None,
    governance_lookup: GovernanceLookup | None = None,
) -> list[CatalogRow]:
    if host is not None and host not in manifest.targets:
        raise ManifestError(f"unknown host: {host}")

    rows: list[CatalogRow] = []
    for skill in manifest.skills.values():
        if enabled is not None and skill.enabled is not enabled:
            continue
        if host is not None and skill.hosts and host not in skill.hosts:
            continue
        rows.append(
            _row_for_skill(
                manifest,
                skill,
                status_hosts,
                status_lookup,
                governance_lookup,
            )
        )
    return rows


def rows_json(rows: list[CatalogRow]) -> CatalogJson:
    return {"skills": [_row_json(row) for row in rows]}


def rows_table(rows: list[CatalogRow], status_hosts: list[str]) -> str:
    include_governance = any(row.audit is not None for row in rows)
    headers = ["SKILL", "ENABLED", *(host.upper() for host in status_hosts)]
    if include_governance:
        headers.extend(["AUDIT", "TRUST"])
    raw_rows = [
        [
            _skill_cell(row),
            "yes" if row.enabled else "no",
            *(_status_cell(row, host) for host in status_hosts),
            *(_governance_cells(row) if include_governance else ()),
        ]
        for row in rows
    ]

    widths = [
        max(len(item) for item in [header, *(raw[index] for raw in raw_rows)])
        for index, header in enumerate(headers)
    ]
    lines = [_format_table_line(headers, widths)]
    lines.append(_format_table_line(["-" * width for width in widths], widths))
    lines.extend(_format_table_line(raw, widths) for raw in raw_rows)
    return "\n".join(lines)


def _row_for_skill(
    manifest: Manifest,
    skill: Skill,
    status_hosts: list[str] | None,
    status_lookup: StatusLookup | None,
    governance_lookup: GovernanceLookup | None,
) -> CatalogRow:
    source_path = manifest.skills_dir / skill.name
    status = None
    if status_hosts is not None and status_lookup is not None:
        status = {host: status_lookup(skill, host) for host in status_hosts}
    audit = None
    provenance = None
    if governance_lookup is not None:
        audit, provenance = governance_lookup(skill)
    error = None
    try:
        description = _description(source_path)
    except ManifestError as exc:
        error = str(exc)
        description = f"invalid: {error}"
    return CatalogRow(
        name=skill.name,
        enabled=skill.enabled,
        hosts=tuple(skill.hosts),
        description=description,
        path=Path(manifest.skills_root) / skill.name,
        status=status,
        audit=audit,
        provenance=provenance,
        error=error,
    )


def _description(path: Path) -> str:
    try:
        metadata, _ = parse_frontmatter((path / "SKILL.md").read_text(encoding="utf-8"))
    except (FileNotFoundError, FrontmatterError) as exc:
        raise ManifestError(f"invalid skill metadata for {path.name}: {exc}") from exc
    return str(metadata.get("description", ""))


def _row_json(row: CatalogRow) -> SkillJsonRow:
    raw: SkillJsonRow = {
        "name": row.name,
        "enabled": row.enabled,
        "hosts": list(row.hosts),
        "description": row.description,
        "path": str(row.path),
    }
    if row.status is not None:
        raw["status"] = {host: status.value for host, status in row.status.items()}
    if row.audit is not None:
        raw["audit"] = row.audit
    if row.provenance is not None:
        raw["provenance"] = row.provenance
    if row.error is not None:
        raw["error"] = row.error
    return raw


def _skill_cell(row: CatalogRow) -> str:
    return f"{row.name} (invalid)" if row.error else row.name


def _status_cell(row: CatalogRow, host: str) -> str:
    """One per-host status cell: ``-`` when the skill does not target the host
    or status was not computed, otherwise the lowercased install status."""
    supported = (not row.hosts) or (host in row.hosts)
    if not supported or row.status is None:
        return "-"
    status = row.status.get(host)
    return status.value.lower() if status is not None else "-"


def _governance_cells(row: CatalogRow) -> tuple[str, str]:
    audit = row.audit or {}
    provenance = row.provenance or {}
    return (audit.get("status", "-"), provenance.get("trust_tier", "-"))


def _format_table_line(items: list[str], widths: list[int]) -> str:
    return "  ".join(
        item.ljust(widths[index]) for index, item in enumerate(items)
    ).rstrip()
