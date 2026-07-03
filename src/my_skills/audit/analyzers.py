from __future__ import annotations

from enum import Enum
from pathlib import Path
from typing import Protocol

from ..ignore import is_ignored
from .models import AuditContext, AuditFinding, AuditResult, Severity
from .policy import AuditPolicy


class AnalyzerScope(str, Enum):
    FILE = "file"
    SKILL = "skill"


class Analyzer(Protocol):
    id: str
    scope: AnalyzerScope

    def analyze(self, context: AuditContext) -> tuple[AuditFinding, ...]:
        ...


TEXT_SUFFIXES = {
    "",
    ".cfg",
    ".ini",
    ".json",
    ".md",
    ".py",
    ".sh",
    ".toml",
    ".txt",
    ".yaml",
    ".yml",
}


def default_analyzers() -> tuple[Analyzer, ...]:
    from .commands import CommandAnalyzer
    from .dataflow import DataflowAnalyzer
    from .markdown import MarkdownAnalyzer
    from .static import StaticAnalyzer
    from .structure import StructureAnalyzer

    return (
        StaticAnalyzer(),
        StructureAnalyzer(),
        MarkdownAnalyzer(),
        CommandAnalyzer(),
        DataflowAnalyzer(),
    )


def run_audit(
    root: Path,
    *,
    policy: AuditPolicy | None = None,
    analyzers: tuple[Analyzer, ...] | None = None,
) -> AuditResult:
    root = Path(root)
    active_policy = policy or AuditPolicy()
    selected = tuple(
        analyzer
        for analyzer in (analyzers or default_analyzers())
        if analyzer.id not in active_policy.disabled_rules
    )
    findings: list[AuditFinding] = []
    if not active_policy.enabled:
        return AuditResult(root.name, root, (), active_policy.profile, None)

    for analyzer in selected:
        if analyzer.scope is AnalyzerScope.SKILL:
            findings.extend(
                _active_findings(
                    _safe_analyze(analyzer, AuditContext(root=root)),
                    active_policy,
                )
            )
        else:
            for context in _file_contexts(root):
                findings.extend(
                    _active_findings(_safe_analyze(analyzer, context), active_policy)
                )

    return AuditResult(
        skill=root.name,
        root=root,
        findings=tuple(findings),
        profile=active_policy.profile,
        threshold=active_policy.effective_threshold,
    )


def _file_contexts(root: Path) -> tuple[AuditContext, ...]:
    contexts: list[AuditContext] = []
    for file in sorted(root.rglob("*")):
        if not file.is_file():
            continue
        relative = file.relative_to(root)
        if is_ignored(relative):
            continue
        rel = str(relative)
        try:
            raw = file.read_bytes()
        except OSError as exc:
            contexts.append(
                AuditContext(
                    root=root,
                    file=file,
                    relative_file=rel,
                    decode_error=True,
                    text=f"read error: {exc}",
                )
            )
            continue
        text = None
        decode_error = False
        if file.suffix.lower() in TEXT_SUFFIXES:
            try:
                text = raw.decode("utf-8")
            except UnicodeDecodeError:
                decode_error = True
        contexts.append(
            AuditContext(
                root=root,
                file=file,
                relative_file=rel,
                raw=raw,
                text=text,
                decode_error=decode_error,
            )
        )
    return tuple(contexts)


def _safe_analyze(analyzer: Analyzer, context: AuditContext) -> tuple[AuditFinding, ...]:
    try:
        return analyzer.analyze(context)
    except OSError as exc:
        return (
            AuditFinding(
                rule_id="audit-read-error",
                category="scanner",
                severity=Severity.CRITICAL,
                file=context.relative_file or "SKILL.md",
                message=f"{analyzer.id} failed: {exc}",
            ),
        )


def _active_findings(
    findings: tuple[AuditFinding, ...],
    policy: AuditPolicy,
) -> tuple[AuditFinding, ...]:
    return tuple(
        finding for finding in findings if finding.rule_id not in policy.disabled_rules
    )
