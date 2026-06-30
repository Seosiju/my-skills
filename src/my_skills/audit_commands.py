from __future__ import annotations

import argparse
import json
import sys

from .audit.analyzers import run_audit
from .audit.bundle import cross_skill_findings
from .audit.formatting import finding_json, result_json
from .audit.gate import audit_policy_from_manifest
from .audit.models import AuditFinding
from .cli_runtime import load_manifest_from_cwd, select_requested
from .config import ManifestError


def cmd_audit(args: argparse.Namespace) -> int:
    try:
        manifest = load_manifest_from_cwd()
        skills, _hosts = select_requested(args, manifest)
    except ManifestError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2

    policy = audit_policy_from_manifest(manifest)
    results = tuple(
        run_audit(manifest.skills_dir / skill.name, policy=policy)
        for skill in skills
    )
    bundle_findings = () if args.skill else cross_skill_findings(results)
    bundle_blocked = _bundle_blocked(bundle_findings, policy.effective_threshold)
    if args.json:
        if args.skill:
            print(json.dumps(result_json(results[0]), indent=2))
        else:
            print(
                json.dumps(
                    {
                        "blocked": any(result.blocked for result in results) or bundle_blocked,
                        "bundle_findings": [
                            finding_json(finding) for finding in bundle_findings
                        ],
                        "skills": [result_json(result) for result in results],
                    },
                    indent=2,
                )
            )
    else:
        for result in results:
            status = "BLOCKED" if result.blocked else "OK"
            threshold = result.threshold.label if result.threshold else "none"
            print(f"[{status}] {result.skill} (threshold={threshold})")
            for finding in result.findings:
                print(
                    f"  {finding.severity.label}: {finding.rule_id}: "
                    f"{finding.file}: {finding.message}"
                )
        if bundle_findings:
            status = "BLOCKED" if bundle_blocked else "WARN"
            print(f"[{status}] selected skill bundle")
            for finding in bundle_findings:
                print(
                    f"  {finding.severity.label}: {finding.rule_id}: "
                    f"{finding.file}: {finding.message}"
                )
    return 1 if any(result.blocked for result in results) or bundle_blocked else 0


def _bundle_blocked(findings: tuple[AuditFinding, ...], threshold) -> bool:
    return threshold is not None and any(finding.severity >= threshold for finding in findings)
