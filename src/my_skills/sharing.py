from __future__ import annotations

import shutil
from datetime import datetime, timezone
from pathlib import Path

from .audit.analyzers import run_audit
from .audit.gate import audit_metadata, audit_policy_from_manifest
from .audit.policy import AuditPolicy
from .checks import compose_validation
from .config import Manifest, ManifestError
from .frontmatter import FrontmatterError, parse_frontmatter
from .hashing import hash_directory
from .manifest_edit import register_skill
from .sharing_formatting import share_plan_json, share_plan_table
from .sharing_models import (
    Risk,
    ShareApplyResult,
    ShareBlockedError,
    ShareCandidate,
    SharePlan,
)
from .state import InstallRecord, State


def plan_share_from_host(manifest: Manifest, from_host: str) -> SharePlan:
    if from_host not in manifest.targets:
        raise ManifestError(f"unknown host: {from_host}")

    source_root = manifest.targets[from_host].path
    candidates = tuple(
        _candidate_for_source(manifest, source)
        for source in sorted(_host_skill_dirs(source_root), key=lambda path: path.name)
    )
    return SharePlan(from_host=from_host, source=source_root, candidates=candidates)


def apply_share_from_host(
    manifest: Manifest,
    manifest_path: Path,
    from_host: str,
    skill: str,
    *,
    enabled: bool,
    force: bool,
    state: State,
    skip_audit: bool = False,
) -> ShareApplyResult:
    if from_host not in manifest.targets:
        raise ManifestError(f"unknown host: {from_host}")

    source = manifest.targets[from_host].path / skill
    if not (source / "SKILL.md").is_file():
        raise ManifestError(f"{source} is not an Agent Skill (no SKILL.md)")

    audit_policy = audit_policy_from_manifest(manifest)
    candidate = _candidate_for_source(
        manifest,
        source,
        audit_policy=audit_policy,
        skip_audit=skip_audit,
    )
    errors = [risk.message for risk in candidate.risks if risk.severity in ("critical", "error")]
    if errors:
        raise ShareBlockedError(
            f"{candidate.name}: validation failed: {'; '.join(errors)}"
        )
    if candidate.canonical_status == "different" and not force:
        raise ShareBlockedError(
            f"{candidate.name}: a different canonical skill already exists at "
            f"{candidate.canonical}; re-run with --force to overwrite it"
        )

    if candidate.canonical_status != "identical":
        _replace_canonical(source, candidate.canonical, force=force)

    result = compose_validation(candidate.canonical)
    if not result.ok:
        raise ShareBlockedError(
            f"{candidate.name}: canonical validation failed after import: "
            f"{'; '.join(result.errors)}"
        )

    hosts = tuple(name for name, target in manifest.targets.items() if target.enabled)
    register_skill(manifest_path, candidate.name, enabled=enabled, hosts=hosts)
    _adopt_source_host(
        state,
        candidate.name,
        from_host,
        candidate.canonical,
        source,
        audit_policy=audit_policy,
    )
    state.save()
    return ShareApplyResult(
        name=candidate.name,
        source=source,
        canonical=candidate.canonical,
        enabled=enabled,
        hosts=hosts,
        adopted_host=from_host,
    )


def _host_skill_dirs(source_root: Path) -> list[Path]:
    if not source_root.is_dir():
        return []
    return [
        child
        for child in source_root.iterdir()
        if child.is_dir() and (child / "SKILL.md").is_file()
    ]


def _candidate_for_source(
    manifest: Manifest,
    source: Path,
    *,
    audit_policy: AuditPolicy | None = None,
    skip_audit: bool = False,
) -> ShareCandidate:
    name, description = _metadata(source)
    canonical = manifest.skills_dir / name
    risks = _risks(
        manifest,
        source,
        audit_policy=audit_policy,
        skip_audit=skip_audit,
    )
    canonical_status = _canonical_status(canonical, source)
    choices, recommended = _choices(risks, canonical_status)
    return ShareCandidate(
        name=name,
        description=description,
        source=source,
        canonical=canonical,
        canonical_status=canonical_status,
        content_hash=hash_directory(source),
        risks=risks,
        choices=choices,
        recommended=recommended,
    )


def _metadata(source: Path) -> tuple[str, str]:
    try:
        meta, _ = parse_frontmatter((source / "SKILL.md").read_text(encoding="utf-8"))
    except (FrontmatterError, OSError):
        return source.name, ""
    return str(meta.get("name") or source.name), str(meta.get("description") or "")


def _risks(
    manifest: Manifest,
    source: Path,
    *,
    audit_policy: AuditPolicy | None = None,
    skip_audit: bool,
) -> tuple[Risk, ...]:
    result = compose_validation(source)
    risks = [Risk("error", message) for message in result.errors]
    risks.extend(Risk("warning", message) for message in result.warnings)
    seen = {_risk_identity(risk.message) for risk in risks}
    if not skip_audit:
        policy = audit_policy or audit_policy_from_manifest(manifest)
        audit = run_audit(source, policy=policy)
        for finding in audit.findings:
            identity = f"{finding.file}: {finding.message}"
            if identity in seen:
                continue
            seen.add(identity)
            risks.append(
                Risk(finding.severity.label, f"{finding.rule_id}: {finding.file}: {finding.message}")
            )
    return tuple(risks)


def _risk_identity(message: str) -> str:
    parts = message.split(": ", 2)
    if len(parts) == 3:
        return f"{parts[1]}: {parts[2]}"
    return message


def _canonical_status(canonical: Path, source: Path) -> str:
    if not canonical.exists():
        return "missing"
    if hash_directory(canonical) == hash_directory(source):
        return "identical"
    return "different"


def _choices(risks: tuple[Risk, ...], canonical_status: str) -> tuple[tuple[str, ...], str]:
    if any(risk.severity in ("critical", "error") for risk in risks) or canonical_status == "different":
        return ("skip",), "skip"
    return ("share-enable", "share-disable", "skip"), "share-enable"


def _replace_canonical(source: Path, canonical: Path, *, force: bool) -> None:
    if canonical.exists():
        if not force:
            raise ShareBlockedError(f"{canonical.name}: canonical skill already exists")
        if canonical.is_dir() and not canonical.is_symlink():
            shutil.rmtree(canonical)
        else:
            canonical.unlink()
    canonical.parent.mkdir(parents=True, exist_ok=True)
    shutil.copytree(source, canonical)


def _adopt_source_host(
    state: State,
    skill: str,
    host: str,
    canonical: Path,
    source: Path,
    *,
    audit_policy: AuditPolicy,
) -> None:
    audit = run_audit(canonical, policy=audit_policy)
    metadata = audit_metadata(audit)
    state.put(
        InstallRecord(
            skill=skill,
            host=host,
            mode="copy",
            source=str(canonical),
            destination=str(source),
            source_hash=hash_directory(canonical),
            installed_hash=hash_directory(source),
            installed_at=_utcnow(),
            source_type="host",
            source_url=str(source),
            source_revision=hash_directory(source),
            **metadata,
        )
    )


def _utcnow() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
