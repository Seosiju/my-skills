from __future__ import annotations

from .gate import AuditGateResult
from .models import AuditFinding, AuditResult


def finding_json(finding: AuditFinding) -> dict:
    payload = {
        "rule_id": finding.rule_id,
        "category": finding.category,
        "severity": finding.severity.label,
        "file": finding.file,
        "message": finding.message,
    }
    if finding.line is not None:
        payload["line"] = finding.line
    return payload


def result_json(result: AuditResult) -> dict:
    return {
        "skill": result.skill,
        "path": str(result.root),
        "profile": result.profile,
        "threshold": result.threshold.label if result.threshold else "none",
        "blocked": result.blocked,
        "result_hash": result.result_hash,
        "findings": [finding_json(finding) for finding in result.findings],
    }


def gate_json(gate: AuditGateResult) -> dict:
    return {
        "skipped": gate.skipped,
        "blocked": gate.blocked,
        "skills": [result_json(result) for result in gate.results],
    }


def format_gate(gate: AuditGateResult, *, blocked_title: str = "AUDIT BLOCKED") -> str:
    if gate.skipped:
        return "WARN: audit skipped by explicit --skip-audit"
    if not gate.results:
        return "Audit: no skills scanned"
    lines: list[str] = []
    if gate.blocked:
        lines.append(blocked_title)
    else:
        lines.append("Audit: passed")
    for result in gate.results:
        status = "blocked" if result.blocked else "ok"
        threshold = result.threshold.label if result.threshold else "none"
        lines.append(f"  {result.skill}: {status} (threshold={threshold})")
        for finding in result.findings:
            lines.append(
                f"    {finding.severity.label}: {finding.rule_id}: "
                f"{finding.file}: {finding.message}"
            )
    return "\n".join(lines)
