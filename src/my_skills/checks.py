from __future__ import annotations

from pathlib import Path

from .security import scan_skill
from .validation import ValidationResult, validate_skill


def compose_validation(skill_dir: Path) -> ValidationResult:
    result = validate_skill(skill_dir)
    for finding in scan_skill(skill_dir):
        line = f"security: {finding.file}: {finding.message}"
        if finding.severity == "error":
            result.errors.append(line)
        else:
            result.warnings.append(line)
    return result
