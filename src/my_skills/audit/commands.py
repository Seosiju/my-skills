from __future__ import annotations

import re

from .analyzers import AnalyzerScope
from .models import AuditContext, AuditFinding, Severity

_CREDENTIAL_READER_RE = re.compile(
    r"(?ix)"
    r"\b(?:cat|less|grep|awk|sed)\b[^\n`]*"
    r"(?:~|\$HOME|/Users/[^/\s]+|/home/[^/\s]+)?/"
    r"(?:\.aws/credentials|\.ssh/id_rsa|\.config/gcloud|\.docker/config\.json)"
    r"|\bgh\s+auth\s+token\b"
    r"|\bsecurity\s+find-(?:generic|internet)-password\b"
    r"|\bop\s+read\b"
)
_NETWORK_SENDER_RE = re.compile(
    r"(?ix)"
    r"\b(?:curl|wget)\b[^\n`]*https?://"
    r"|\b(?:nc|netcat)\b[^\n`]*(?:\d{1,3}(?:\.\d{1,3}){3}|[a-z0-9.-]+\.[a-z]{2,})"
    r"|\b(?:scp|rsync|ssh)\b[^\n`]*(?:[a-z0-9._-]+@|[a-z0-9.-]+\.[a-z]{2,})"
)


class CommandAnalyzer:
    id = "command-tier"
    scope = AnalyzerScope.FILE

    def analyze(self, context: AuditContext) -> tuple[AuditFinding, ...]:
        text = context.text
        if text is None:
            return ()
        rel = context.relative_file or "SKILL.md"
        findings: list[AuditFinding] = []
        findings.extend(_command_findings(text, rel, "credential-reader", _CREDENTIAL_READER_RE))
        findings.extend(_command_findings(text, rel, "network-sender", _NETWORK_SENDER_RE))
        return tuple(findings)


def contains_credential_reader(text: str) -> bool:
    return bool(_CREDENTIAL_READER_RE.search(text))


def contains_network_sender(text: str) -> bool:
    return bool(_NETWORK_SENDER_RE.search(text))


def _command_findings(
    text: str,
    rel: str,
    rule_id: str,
    pattern: re.Pattern[str],
) -> tuple[AuditFinding, ...]:
    severity = Severity.HIGH if rule_id == "credential-reader" else Severity.MEDIUM
    category = "credential" if rule_id == "credential-reader" else "network"
    message = (
        "command reads credential material"
        if rule_id == "credential-reader"
        else "command can send data over the network"
    )
    return tuple(
        AuditFinding(
            rule_id=rule_id,
            category=category,
            severity=severity,
            file=rel,
            message=message,
            line=_line_number(text, match.start()),
        )
        for match in pattern.finditer(text)
    )


def _line_number(text: str, offset: int) -> int:
    return text.count("\n", 0, offset) + 1
