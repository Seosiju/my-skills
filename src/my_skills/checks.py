from __future__ import annotations

from pathlib import Path

from .audit.analyzers import run_audit
from .audit.models import Severity
from .audit.policy import AuditPolicy
from .validation import ValidationResult, validate_skill


VALIDATION_AUDIT_POLICY = AuditPolicy(enabled=True, disabled_rules=frozenset())


def compose_validation(skill_dir: Path) -> ValidationResult:
    result = validate_skill(skill_dir)
    for finding in run_audit(skill_dir, policy=VALIDATION_AUDIT_POLICY).findings:
        line = f"security: {finding.file}: {finding.message}"
        if finding.severity >= Severity.HIGH:
            result.errors.append(line)
        else:
            result.warnings.append(line)
    return result
