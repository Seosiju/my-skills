from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path

from .analyzers import run_audit
from .models import AuditResult
from .policy import AuditPolicy, policy_from_config


@dataclass(frozen=True, slots=True)
class AuditGateResult:
    results: tuple[AuditResult, ...]
    skipped: bool = False

    @property
    def blocked(self) -> bool:
        return any(result.blocked for result in self.results)

    @property
    def blocked_results(self) -> tuple[AuditResult, ...]:
        return tuple(result for result in self.results if result.blocked)


def audit_skills(
    skills: tuple[Path, ...],
    *,
    policy: AuditPolicy,
    skip: bool = False,
) -> AuditGateResult:
    if skip or not policy.enabled:
        return AuditGateResult((), skipped=True)
    return AuditGateResult(tuple(run_audit(skill, policy=policy) for skill in skills))


def audit_policy_from_manifest(manifest) -> AuditPolicy:
    return policy_from_config(getattr(manifest, "audit", None))


def audit_metadata(result: AuditResult) -> dict[str, str]:
    return {
        "last_audit_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "last_audit_result_hash": result.result_hash,
        "audit_profile": result.profile,
        "audit_threshold": result.threshold.label if result.threshold else "none",
    }
