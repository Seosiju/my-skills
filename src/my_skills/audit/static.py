from __future__ import annotations

import re

from .analyzers import AnalyzerScope
from .models import AuditContext, AuditFinding, Severity

_BIDI_CHARS = frozenset(
    chr(cp)
    for cp in (
        0x202A,
        0x202B,
        0x202C,
        0x202D,
        0x202E,
        0x2066,
        0x2067,
        0x2068,
        0x2069,
        0x200E,
        0x200F,
    )
)
_PRIVATE_KEY_RE = re.compile(r"-----BEGIN [A-Z0-9 ]*PRIVATE KEY-----")
_AWS_KEY_RE = re.compile(r"\bAKIA[0-9A-Z]{16}\b")
_SECRET_ASSIGN_RE = re.compile(
    r"(?i)\b(?:api[_-]?key|secret|access[_-]?token|auth[_-]?token|password|passwd)\b"
    r"\s*[:=]\s*['\"]?[A-Za-z0-9_\-/+]{8,}"
)
_ABS_USER_PATH_RE = re.compile(r"/Users/[^/\s]+|/home/[^/\s]+")
_PROMPT_INJECTION_RE = re.compile(
    r"(?i)\b(ignore|override|forget)\s+(all\s+)?(previous|prior|system)\s+instructions\b"
)
_OUTPUT_SUPPRESSION_RE = re.compile(
    r"(?i)\b(do not|don't)\s+(tell|mention|report|show)\s+(the\s+)?user\b"
)
_DESTRUCTIVE_RE = re.compile(r"\brm\s+-rf\s+(?:/|\$HOME|~|[.]{1,2}/)")
_FETCH_SECRET_RE = re.compile(
    r"(?i)\b(?:curl|wget)\b.*\b(?:token|secret|password|api[_-]?key)\b"
)


class StaticAnalyzer:
    id = "static"
    scope = AnalyzerScope.FILE

    def analyze(self, context: AuditContext) -> tuple[AuditFinding, ...]:
        rel = context.relative_file or "SKILL.md"
        findings: list[AuditFinding] = []
        raw = context.raw or b""
        if b"\x00" in raw:
            findings.append(_finding("nul-byte", "encoding", Severity.CRITICAL, rel, "file contains a NUL byte"))
        if context.decode_error:
            findings.append(
                _finding("encoding", "encoding", Severity.CRITICAL, rel, "file is not valid UTF-8")
            )
            return tuple(findings)
        text = context.text
        if text is None:
            return tuple(findings)
        if any(ch in _BIDI_CHARS for ch in text):
            findings.append(
                _finding(
                    "bidi-unicode",
                    "stealth",
                    Severity.CRITICAL,
                    rel,
                    "hidden/bidirectional Unicode control character",
                )
            )
        if _PRIVATE_KEY_RE.search(text):
            findings.append(_finding("private-key", "credential", Severity.CRITICAL, rel, "embedded private key header"))
        if _AWS_KEY_RE.search(text):
            findings.append(_finding("aws-key", "credential", Severity.CRITICAL, rel, "AWS access key id pattern"))
        if _SECRET_ASSIGN_RE.search(text):
            findings.append(_finding("secret", "credential", Severity.CRITICAL, rel, "secret-like assignment"))
        if _ABS_USER_PATH_RE.search(text):
            findings.append(_finding("abs-user-path", "privacy", Severity.HIGH, rel, "absolute user/home path leak"))
        if _PROMPT_INJECTION_RE.search(text):
            findings.append(_finding("prompt-injection", "prompt-injection", Severity.CRITICAL, rel, "prompt-injection instruction"))
        if _OUTPUT_SUPPRESSION_RE.search(text):
            findings.append(_finding("output-suppression", "prompt-injection", Severity.HIGH, rel, "output suppression instruction"))
        if _DESTRUCTIVE_RE.search(text):
            findings.append(_finding("destructive-command", "command", Severity.HIGH, rel, "destructive shell command"))
        if _FETCH_SECRET_RE.search(text):
            findings.append(_finding("suspicious-fetch", "network", Severity.HIGH, rel, "network command mentions credential material"))
        return tuple(findings)


def _finding(
    rule_id: str,
    category: str,
    severity: Severity,
    file: str,
    message: str,
) -> AuditFinding:
    return AuditFinding(
        rule_id=rule_id,
        category=category,
        severity=severity,
        file=file,
        message=message,
    )
