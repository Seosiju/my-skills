from __future__ import annotations

from .models import AuditFinding, AuditResult, Severity


def cross_skill_findings(results: tuple[AuditResult, ...]) -> tuple[AuditFinding, ...]:
    readers = _skills_with(results, "credential-reader")
    senders = _skills_with(results, "network-sender")
    if not readers or not senders:
        return ()
    if not any(reader != sender for reader in readers for sender in senders):
        return ()
    return (
        AuditFinding(
            rule_id="cross-skill-credential-network",
            category="bundle",
            severity=Severity.CRITICAL,
            file="<bundle>",
            message=(
                "selected skills combine credential readers "
                f"({', '.join(readers)}) with network senders ({', '.join(senders)})"
            ),
        ),
    )


def _skills_with(results: tuple[AuditResult, ...], rule_id: str) -> tuple[str, ...]:
    return tuple(
        sorted(
            result.skill
            for result in results
            if any(finding.rule_id == rule_id for finding in result.findings)
        )
    )
