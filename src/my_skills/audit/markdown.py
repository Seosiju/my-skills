from __future__ import annotations

import re

from .analyzers import AnalyzerScope
from .models import AuditContext, AuditFinding, Severity

_REMOTE_IMAGE_RE = re.compile(r"!\[[^\]]*]\((https?://[^)\s]+)\)")


class MarkdownAnalyzer:
    id = "markdown"
    scope = AnalyzerScope.FILE

    def analyze(self, context: AuditContext) -> tuple[AuditFinding, ...]:
        text = context.text
        rel = context.relative_file or "SKILL.md"
        if text is None or not rel.lower().endswith(".md"):
            return ()
        return tuple(
            AuditFinding(
                rule_id="markdown-remote-image",
                category="markdown",
                severity=Severity.MEDIUM,
                file=rel,
                message=f"markdown image loads remote URL: {match.group(1)}",
                line=_line_number(text, match.start()),
            )
            for match in _REMOTE_IMAGE_RE.finditer(text)
        )


def _line_number(text: str, offset: int) -> int:
    return text.count("\n", 0, offset) + 1
