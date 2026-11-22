"""Regenerate the Rust audit parity data with uv run python tests/golden/audit/generate.py."""

import json
import os
from pathlib import Path
from tempfile import TemporaryDirectory
from unittest.mock import patch

import yaml

from my_skills.audit import AuditFinding, AuditPolicy, AuditResult, Severity, run_audit
from my_skills.audit.bundle import cross_skill_findings
from my_skills.audit.formatting import finding_json, format_gate, gate_json, result_json
from my_skills.audit.gate import audit_skills
from my_skills.audit.models import severity_from_text

ROOT = Path(__file__).resolve().parents[3]
FIXTURES = Path("crates/my-skills/tests/fixtures/audit")
OUTPUT = Path(__file__).with_name("golden.json").resolve()


def scan_case(path):
    baseline = result_json(run_audit(path))
    baseline["path"] = path.as_posix()
    findings = baseline.pop("findings")
    result_hash = baseline.pop("result_hash")
    profiles = {"implicit-default": baseline}
    for profile in ("default", "strict", "permissive"):
        result = result_json(run_audit(path, policy=AuditPolicy(profile=profile)))
        result["path"] = path.as_posix()
        # Profiles affect gating only. Store one copy of the shared payload.
        assert result.pop("findings") == findings
        assert result.pop("result_hash") == result_hash
        profiles[profile] = result
    return {
        "path": path.as_posix(),
        "findings": findings,
        "result_hash": result_hash,
        "profiles": profiles,
    }


def bundle_case(*names):
    results = tuple(run_audit(FIXTURES / name) for name in names)
    return {
        "paths": [(FIXTURES / name).as_posix() for name in names],
        "findings": [finding_json(finding) for finding in cross_skill_findings(results)],
    }


def gate_case(
    *names, profile="default", threshold=None, enabled=True, disabled_rules=(), skip=False
):
    paths = tuple(FIXTURES / name for name in names)
    policy = AuditPolicy(
        profile=profile,
        threshold=severity_from_text(threshold),
        enabled=enabled,
        disabled_rules=frozenset(disabled_rules),
    )
    gate = audit_skills(paths, policy=policy, skip=skip)
    blocked_title = "FIXTURE BLOCKED"
    payload = gate_json(gate)
    for result, path in zip(payload["skills"], paths):
        result["path"] = path.as_posix()
    return {
        "paths": [path.as_posix() for path in paths],
        "settings": {
            "profile": profile,
            "threshold": threshold,
            "enabled": enabled,
            "disabled_rules": disabled_rules,
        },
        "skip": skip,
        "blocked_title": blocked_title,
        "text": format_gate(gate, blocked_title=blocked_title),
        "json": payload,
    }


def unicode_hash_case():
    findings = (
        AuditFinding(
            "synthetic",
            "encoding",
            Severity.INFO,
            "한글😀.md",
            '안녕 😀 "quote" \\ slash / \t \n \r \b \f \x00 \x1f \x7f \u2028',
            7,
        ),
        AuditFinding("synthetic", "encoding", Severity.INFO, "example.md", "No line"),
    )
    result = AuditResult("synthetic", Path("synthetic"), findings, "default", None)
    return {
        "findings": [finding_json(finding) for finding in findings],
        "result_hash": result.result_hash,
    }


def generate():
    paths = sorted(
        path.parent
        for base in (Path("tests/fixtures"), FIXTURES)
        for path in base.glob("*/SKILL.md")
    )
    scans = [scan_case(path) for path in paths]
    bundles = [
        bundle_case("credential-network-flow"),
        bundle_case("credential-reader", "network-sender"),
        bundle_case("regex-parity", "credential-reader", "network-sender", "credential-reader"),
    ]
    builtin_rules = yaml.safe_load(
        Path("src/my_skills/audit/builtin_rules.yaml").read_text(encoding="utf-8")
    )["rules"]
    covered = {finding["rule_id"] for case in scans + bundles for finding in case["findings"]}
    assert {rule["id"] for rule in builtin_rules} <= covered
    return {
        "builtin_rules": builtin_rules,
        "scans": scans,
        "bundles": bundles,
        "gates": [
            gate_case("credential-reader", "clean-skill", profile="strict"),
            gate_case("path-traversal", profile="permissive"),
            gate_case("credential-reader", threshold="  MeDIuM  "),
            gate_case("credential-network-flow", disabled_rules=("command-tier",)),
            gate_case("credential-network-flow", disabled_rules=("credential-reader", "dataflow")),
            gate_case("path-traversal", skip=True),
            gate_case("path-traversal", enabled=False),
            gate_case(),
        ],
        "unicode_hash": unicode_hash_case(),
    }


if __name__ == "__main__":
    os.chdir(ROOT)
    with TemporaryDirectory(prefix="my-skills-audit-golden-") as temp:
        env = {
            name: str(Path(temp) / name.lower())
            for name in ("HOME", "USERPROFILE", "XDG_STATE_HOME", "XDG_DATA_HOME", "XDG_CONFIG_HOME")
        }
        with patch.dict(os.environ, env):
            OUTPUT.write_text(json.dumps(generate(), indent=2) + "\n", encoding="utf-8", newline="\n")
