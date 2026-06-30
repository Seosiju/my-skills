from __future__ import annotations

from pathlib import Path

from .analyzers import AnalyzerScope, TEXT_SUFFIXES
from .commands import contains_credential_reader, contains_network_sender
from .models import AuditContext, AuditFinding, Severity


class DataflowAnalyzer:
    id = "dataflow"
    scope = AnalyzerScope.SKILL

    def analyze(self, context: AuditContext) -> tuple[AuditFinding, ...]:
        readers: list[str] = []
        senders: list[str] = []
        for rel, text in _text_files(context.root):
            if contains_credential_reader(text):
                readers.append(rel)
            if contains_network_sender(text):
                senders.append(rel)
        if not readers or not senders:
            return ()
        return (
            AuditFinding(
                rule_id="credential-network-flow",
                category="dataflow",
                severity=Severity.CRITICAL,
                file="<skill>",
                message=(
                    "skill bundle combines credential readers "
                    f"({', '.join(readers)}) with network senders ({', '.join(senders)})"
                ),
            ),
        )


def _text_files(root: Path) -> tuple[tuple[str, str], ...]:
    files: list[tuple[str, str]] = []
    for file in sorted(root.rglob("*")):
        if not file.is_file() or file.suffix.lower() not in TEXT_SUFFIXES:
            continue
        try:
            text = file.read_text(encoding="utf-8")
        except UnicodeDecodeError:
            continue
        files.append((str(file.relative_to(root)), text))
    return tuple(files)
