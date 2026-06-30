from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass
from enum import IntEnum
from pathlib import Path


class Severity(IntEnum):
    INFO = 10
    LOW = 20
    MEDIUM = 30
    HIGH = 40
    CRITICAL = 50

    @property
    def label(self) -> str:
        return self.name.lower()


def severity_from_text(value: str | None) -> Severity | None:
    if value is None:
        return None
    try:
        return Severity[value.strip().upper()]
    except KeyError as exc:
        raise ValueError(f"unknown audit severity: {value}") from exc


@dataclass(frozen=True, slots=True)
class AuditContext:
    root: Path
    file: Path | None = None
    relative_file: str | None = None
    raw: bytes | None = None
    text: str | None = None
    decode_error: bool = False


@dataclass(frozen=True, slots=True)
class AuditFinding:
    rule_id: str
    category: str
    severity: Severity
    file: str
    message: str
    line: int | None = None


@dataclass(frozen=True, slots=True)
class AuditResult:
    skill: str
    root: Path
    findings: tuple[AuditFinding, ...]
    profile: str
    threshold: Severity | None

    @property
    def blocked(self) -> bool:
        if self.threshold is None:
            return False
        return any(finding.severity >= self.threshold for finding in self.findings)

    @property
    def result_hash(self) -> str:
        payload = [
            {
                "rule_id": finding.rule_id,
                "category": finding.category,
                "severity": finding.severity.label,
                "file": finding.file,
                "message": finding.message,
                "line": finding.line,
            }
            for finding in self.findings
        ]
        raw = json.dumps(payload, sort_keys=True, separators=(",", ":")).encode()
        return "sha256:" + hashlib.sha256(raw).hexdigest()
