from __future__ import annotations

import re

from .analyzers import AnalyzerScope
from .models import AuditContext, AuditFinding, Severity

_TRAVERSAL_RE = re.compile(r"(?<![\w/])(?:\.\./)+[^\s)>\]'\"]*")


class StructureAnalyzer:
    id = "structure"
    scope = AnalyzerScope.FILE

    def analyze(self, context: AuditContext) -> tuple[AuditFinding, ...]:
        text = context.text
        if text is None:
            return ()
        rel = context.relative_file or "SKILL.md"
        findings: list[AuditFinding] = []
        for match in _TRAVERSAL_RE.finditer(text):
            target = match.group(0)
            findings.append(
                AuditFinding(
                    rule_id="path-traversal",
                    category="traversal",
                    severity=_severity_for_target(target),
                    file=rel,
                    message=f"relative path escapes the skill directory: {target}",
                )
            )
        return tuple(findings)


def _severity_for_target(target: str) -> Severity:
    lowered = target.lower()
    if ".ssh" in lowered or ".aws" in lowered or "credential" in lowered or "secret" in lowered:
        return Severity.CRITICAL
    return Severity.CRITICAL
