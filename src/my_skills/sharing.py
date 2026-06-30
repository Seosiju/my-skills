from __future__ import annotations

import shutil
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import TypedDict

from .audit.analyzers import run_audit
from .audit.gate import audit_policy_from_manifest
from .checks import compose_validation
from .config import Manifest, ManifestError
from .frontmatter import FrontmatterError, parse_frontmatter
from .hashing import hash_directory
from .manifest_edit import register_skill
from .state import InstallRecord, State


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


class ShareBlockedError(ValueError):
    pass


@dataclass(frozen=True, slots=True)
class ShareApplyResult:
    name: str
    source: Path
    canonical: Path
    enabled: bool
    hosts: tuple[str, ...]
    adopted_host: str


class RiskJson(TypedDict):
    severity: str
    message: str


class ShareCandidateJson(TypedDict):
    name: str
    description: str
    source: str
    canonical: str
    canonical_status: str
    content_hash: str
    risks: list[RiskJson]
    choices: list[str]
    recommended: str


SharePlanJson = TypedDict(
    "SharePlanJson",
    {"from": str, "source": str, "candidates": list[ShareCandidateJson]},
)


def plan_share_from_host(manifest: Manifest, from_host: str) -> SharePlan:
    if from_host not in manifest.targets:
        raise ManifestError(f"unknown host: {from_host}")

    source_root = manifest.targets[from_host].path
    candidates = tuple(
        _candidate_for_source(manifest, source)
        for source in sorted(_host_skill_dirs(source_root), key=lambda path: path.name)
    )
    return SharePlan(from_host=from_host, source=source_root, candidates=candidates)


def share_plan_json(plan: SharePlan) -> SharePlanJson:
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


def apply_share_from_host(
    manifest: Manifest,
    manifest_path: Path,
    from_host: str,
    skill: str,
    *,
    enabled: bool,
    force: bool,
    state: State,
    skip_audit: bool = False,
) -> ShareApplyResult:
    if from_host not in manifest.targets:
        raise ManifestError(f"unknown host: {from_host}")

    source = manifest.targets[from_host].path / skill
    if not (source / "SKILL.md").is_file():
        raise ManifestError(f"{source} is not an Agent Skill (no SKILL.md)")

    candidate = _candidate_for_source(manifest, source, skip_audit=skip_audit)
    errors = [risk.message for risk in candidate.risks if risk.severity in ("critical", "error")]
    if errors:
        raise ShareBlockedError(
            f"{candidate.name}: validation failed: {'; '.join(errors)}"
        )
    if candidate.canonical_status == "different" and not force:
        raise ShareBlockedError(
            f"{candidate.name}: a different canonical skill already exists at "
            f"{candidate.canonical}; re-run with --force to overwrite it"
        )

    if candidate.canonical_status != "identical":
        _replace_canonical(source, candidate.canonical, force=force)

    result = compose_validation(candidate.canonical)
    if not result.ok:
        raise ShareBlockedError(
            f"{candidate.name}: canonical validation failed after import: "
            f"{'; '.join(result.errors)}"
        )

    hosts = tuple(name for name, target in manifest.targets.items() if target.enabled)
    register_skill(manifest_path, candidate.name, enabled=enabled, hosts=hosts)
    _adopt_source_host(state, candidate.name, from_host, candidate.canonical, source)
    state.save()
    return ShareApplyResult(
        name=candidate.name,
        source=source,
        canonical=candidate.canonical,
        enabled=enabled,
        hosts=hosts,
        adopted_host=from_host,
    )


def _host_skill_dirs(source_root: Path) -> list[Path]:
    if not source_root.is_dir():
        return []
    return [
        child
        for child in source_root.iterdir()
        if child.is_dir() and (child / "SKILL.md").is_file()
    ]


def _candidate_for_source(manifest: Manifest, source: Path, *, skip_audit: bool = False) -> ShareCandidate:
    name, description = _metadata(source)
    canonical = manifest.skills_dir / name
    risks = _risks(manifest, source, skip_audit=skip_audit)
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


def _risks(manifest: Manifest, source: Path, *, skip_audit: bool) -> tuple[Risk, ...]:
    result = compose_validation(source)
    risks = [Risk("error", message) for message in result.errors]
    risks.extend(Risk("warning", message) for message in result.warnings)
    if not skip_audit:
        audit = run_audit(source, policy=audit_policy_from_manifest(manifest))
        for finding in audit.findings:
            risks.append(
                Risk(finding.severity.label, f"{finding.rule_id}: {finding.file}: {finding.message}")
            )
    return tuple(risks)


def _canonical_status(canonical: Path, source: Path) -> str:
    if not canonical.exists():
        return "missing"
    if hash_directory(canonical) == hash_directory(source):
        return "identical"
    return "different"


def _choices(risks: tuple[Risk, ...], canonical_status: str) -> tuple[tuple[str, ...], str]:
    if any(risk.severity in ("critical", "error") for risk in risks) or canonical_status == "different":
        return ("skip",), "skip"
    return ("share-enable", "share-disable", "skip"), "share-enable"


def _risk_json(risk: Risk) -> RiskJson:
    return {"severity": risk.severity, "message": risk.message}


def _candidate_json(candidate: ShareCandidate) -> ShareCandidateJson:
    return {
        "name": candidate.name,
        "description": candidate.description,
        "source": str(candidate.source),
        "canonical": str(candidate.canonical),
        "canonical_status": candidate.canonical_status,
        "content_hash": candidate.content_hash,
        "risks": [_risk_json(risk) for risk in candidate.risks],
        "choices": list(candidate.choices),
        "recommended": candidate.recommended,
    }


def _risk_summary(risks: tuple[Risk, ...]) -> str:
    if not risks:
        return "none"
    return ", ".join(f"{risk.severity}:{risk.message}" for risk in risks)


def _format_table_line(items: list[str], widths: list[int]) -> str:
    return "  ".join(item.ljust(widths[index]) for index, item in enumerate(items))


def _replace_canonical(source: Path, canonical: Path, *, force: bool) -> None:
    if canonical.exists():
        if not force:
            raise ShareBlockedError(f"{canonical.name}: canonical skill already exists")
        if canonical.is_dir() and not canonical.is_symlink():
            shutil.rmtree(canonical)
        else:
            canonical.unlink()
    canonical.parent.mkdir(parents=True, exist_ok=True)
    shutil.copytree(source, canonical)


def _adopt_source_host(
    state: State,
    skill: str,
    host: str,
    canonical: Path,
    source: Path,
) -> None:
    state.put(
        InstallRecord(
            skill=skill,
            host=host,
            mode="copy",
            source=str(canonical),
            destination=str(source),
            source_hash=hash_directory(canonical),
            installed_hash=hash_directory(source),
            installed_at=_utcnow(),
        )
    )


def _utcnow() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
