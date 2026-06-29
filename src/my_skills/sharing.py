from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Any

from .checks import compose_validation
from .config import Manifest, ManifestError
from .frontmatter import FrontmatterError, parse_frontmatter
from .hashing import hash_directory


@dataclass(frozen=True, slots=True)
class Risk:
    severity: str
    message: str


@dataclass(frozen=True, slots=True)
class ShareCandidate:
    name: str
    description: str
    source: Path
    canonical: Path
    canonical_status: str
    content_hash: str
    risks: tuple[Risk, ...]
    choices: tuple[str, ...]
    recommended: str


@dataclass(frozen=True, slots=True)
class SharePlan:
    from_host: str
    source: Path
    candidates: tuple[ShareCandidate, ...]


def plan_share_from_host(manifest: Manifest, from_host: str) -> SharePlan:
    if from_host not in manifest.targets:
        raise ManifestError(f"unknown host: {from_host}")

    source_root = manifest.targets[from_host].path
    candidates = tuple(
        _candidate_for_source(manifest, source)
        for source in sorted(_host_skill_dirs(source_root), key=lambda path: path.name)
    )
    return SharePlan(from_host=from_host, source=source_root, candidates=candidates)


def share_plan_json(plan: SharePlan) -> dict[str, Any]:
    return {
        "from": plan.from_host,
        "source": str(plan.source),
        "candidates": [_candidate_json(candidate) for candidate in plan.candidates],
    }


def share_plan_table(plan: SharePlan) -> str:
    if not plan.candidates:
        return f"no skills found in {plan.source}"

    headers = ["Skill", "Canonical", "Recommended", "Risks"]
    rows = [
        [
            candidate.name,
            candidate.canonical_status,
            candidate.recommended,
            _risk_summary(candidate.risks),
        ]
        for candidate in plan.candidates
    ]
    widths = [
        max(len(item) for item in [header, *(row[index] for row in rows)])
        for index, header in enumerate(headers)
    ]
    lines = [_format_table_line(headers, widths)]
    lines.append(_format_table_line(["-" * width for width in widths], widths))
    lines.extend(_format_table_line(row, widths) for row in rows)
    return "\n".join(lines)


def _host_skill_dirs(source_root: Path) -> list[Path]:
    if not source_root.is_dir():
        return []
    return [
        child
        for child in source_root.iterdir()
        if child.is_dir() and (child / "SKILL.md").is_file()
    ]


def _candidate_for_source(manifest: Manifest, source: Path) -> ShareCandidate:
    name, description = _metadata(source)
    canonical = manifest.skills_dir / name
    risks = _risks(source)
    canonical_status = _canonical_status(canonical, source)
    choices, recommended = _choices(risks, canonical_status)
    return ShareCandidate(
        name=name,
        description=description,
        source=source,
        canonical=canonical,
        canonical_status=canonical_status,
        content_hash=hash_directory(source),
        risks=risks,
        choices=choices,
        recommended=recommended,
    )


def _metadata(source: Path) -> tuple[str, str]:
    try:
        meta, _ = parse_frontmatter((source / "SKILL.md").read_text(encoding="utf-8"))
    except (FrontmatterError, OSError):
        return source.name, ""
    return str(meta.get("name") or source.name), str(meta.get("description") or "")


def _risks(source: Path) -> tuple[Risk, ...]:
    result = compose_validation(source)
    return tuple(
        [Risk("error", message) for message in result.errors]
        + [Risk("warning", message) for message in result.warnings]
    )


def _canonical_status(canonical: Path, source: Path) -> str:
    if not canonical.exists():
        return "missing"
    if hash_directory(canonical) == hash_directory(source):
        return "identical"
    return "different"


def _choices(risks: tuple[Risk, ...], canonical_status: str) -> tuple[tuple[str, ...], str]:
    if any(risk.severity == "error" for risk in risks) or canonical_status == "different":
        return ("skip",), "skip"
    return ("share-enable", "share-disable", "skip"), "share-enable"


def _candidate_json(candidate: ShareCandidate) -> dict[str, Any]:
    return {
        "name": candidate.name,
        "description": candidate.description,
        "source": str(candidate.source),
        "canonical": str(candidate.canonical),
        "canonical_status": candidate.canonical_status,
        "content_hash": candidate.content_hash,
        "risks": [
            {"severity": risk.severity, "message": risk.message}
            for risk in candidate.risks
        ],
        "choices": list(candidate.choices),
        "recommended": candidate.recommended,
    }


def _risk_summary(risks: tuple[Risk, ...]) -> str:
    if not risks:
        return "none"
    return ", ".join(f"{risk.severity}:{risk.message}" for risk in risks)


def _format_table_line(items: list[str], widths: list[int]) -> str:
    return "  ".join(item.ljust(widths[index]) for index, item in enumerate(items))
