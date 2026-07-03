from __future__ import annotations

from typing import TypedDict

from .sharing_models import Risk, ShareCandidate, SharePlan


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
