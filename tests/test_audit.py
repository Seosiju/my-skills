import json
from pathlib import Path

from my_skills import cli
from my_skills.audit.analyzers import Analyzer, AnalyzerScope, run_audit
from my_skills.audit.models import AuditContext, AuditFinding, Severity
from my_skills.audit.policy import AuditPolicy
from my_skills.state import State


def _repo(tmp_path: Path, body: str = "# Alpha\n") -> tuple[Path, Path, Path]:
    target = tmp_path / "hosts" / "hermes"
    (tmp_path / "my-skills.toml").write_text(
        'schema_version = 1\nskills_root = "skills"\n\n'
        f'[targets.hermes]\nenabled = true\nscope = "user"\npath = "{target}"\n\n'
        '[skills.alpha]\nenabled = true\nhosts = ["hermes"]\n'
    )
    skill = tmp_path / "skills" / "alpha"
    skill.mkdir(parents=True)
    skill.joinpath("SKILL.md").write_text(
        "---\nname: alpha\ndescription: A valid skill for audit tests.\n---\n\n"
        f"{body}"
    )
    return tmp_path, target, skill


def _external_skill(tmp_path: Path, body: str) -> Path:
    skill = tmp_path / "external" / "alpha"
    skill.mkdir(parents=True)
    skill.joinpath("SKILL.md").write_text(
        "---\nname: alpha\ndescription: A valid external skill.\n---\n\n"
        f"{body}"
    )
    return skill


def test_audit_cli_json_reports_blocking_finding(tmp_path, monkeypatch, capsys):
    repo, _target, _skill = _repo(tmp_path, "Read ../secret.txt before starting.\n")
    monkeypatch.chdir(repo)

    rc = cli.main(["audit", "alpha", "--json"])

    payload = json.loads(capsys.readouterr().out)
    assert rc == 1
    assert payload["skill"] == "alpha"
    assert payload["blocked"] is True
    assert payload["threshold"] == "critical"
    assert payload["findings"][0]["rule_id"] == "path-traversal"


def test_install_dry_run_json_includes_audit_would_block(tmp_path, monkeypatch, capsys):
    repo, target, _skill = _repo(tmp_path, "Load ../../private/config.json.\n")
    monkeypatch.chdir(repo)
    monkeypatch.setenv("XDG_STATE_HOME", str(tmp_path / "state"))

    rc = cli.main(["install", "alpha", "--host", "hermes", "--dry-run", "--json"])

    payload = json.loads(capsys.readouterr().out)
    assert rc == 0
    assert payload["audit"]["blocked"] is True
    assert payload["audit"]["skills"][0]["findings"][0]["rule_id"] == "path-traversal"
    assert payload["actions"][0]["action"] == "CREATE"
    assert not (target / "alpha").exists()


def test_install_apply_blocks_audit_failure_before_write(tmp_path, monkeypatch, capsys):
    repo, target, _skill = _repo(tmp_path, "Load ../private/config.json.\n")
    monkeypatch.chdir(repo)
    monkeypatch.setenv("XDG_STATE_HOME", str(tmp_path / "state"))

    assert cli.main(["install", "alpha", "--host", "hermes"]) == 1

    out = capsys.readouterr().out
    assert "AUDIT BLOCKED" in out
    assert "path-traversal" in out
    assert not (target / "alpha").exists()


def test_import_blocks_traversal_before_canonical_write(tmp_path, monkeypatch, capsys):
    repo, _target, _skill = _repo(tmp_path)
    external = _external_skill(tmp_path, "Use ../private/config.json.\n")
    monkeypatch.chdir(repo)

    assert cli.main(["import", str(external), "--force"]) == 1

    assert "AUDIT BLOCKED" in capsys.readouterr().out
    assert "Use ../private" not in (repo / "skills" / "alpha" / "SKILL.md").read_text()


def test_skip_audit_is_explicit_and_allows_write(tmp_path, monkeypatch, capsys):
    repo, target, _skill = _repo(tmp_path, "Load ../private/config.json.\n")
    monkeypatch.chdir(repo)
    monkeypatch.setenv("XDG_STATE_HOME", str(tmp_path / "state"))

    assert cli.main(["install", "alpha", "--host", "hermes", "--skip-audit"]) == 0

    out = capsys.readouterr().out
    assert "audit skipped" in out.lower()
    assert (target / "alpha" / "SKILL.md").exists()


def test_install_records_last_audit_metadata(tmp_path, monkeypatch, capsys):
    repo, _target, _skill = _repo(tmp_path)
    state_root = tmp_path / "state"
    monkeypatch.chdir(repo)
    monkeypatch.setenv("XDG_STATE_HOME", str(state_root))

    assert cli.main(["install", "alpha", "--host", "hermes"]) == 0

    record = State.load().get("alpha", "hermes")
    assert record is not None
    assert record.source_type == "canonical"
    assert record.last_audit_at
    assert record.last_audit_result_hash.startswith("sha256:")
    assert record.audit_profile == "default"
    assert record.audit_threshold == "critical"


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
