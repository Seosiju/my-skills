from pathlib import Path

import pytest

from my_skills import cli
from my_skills.config import load_manifest
from my_skills.manifest_edit import ManifestEditError, register_skill


def _repo(tmp_path: Path) -> Path:
    (tmp_path / "my-skills.toml").write_text(
        "schema_version = 1\n"
        'skills_root = "skills"\n\n'
        "[targets.claude]\n"
        "enabled = true\n"
        f'path = "{tmp_path / "hosts" / "claude"}"\n\n'
        "[targets.codex]\n"
        "enabled = true\n"
        f'path = "{tmp_path / "hosts" / "codex"}"\n\n'
        "[skills.alpha]\n"
        "enabled = true\n"
        'hosts = ["claude"]\n'
    )
    return tmp_path


def test_disable_and_enable_update_skill_enabled(tmp_path, monkeypatch, capsys):
    repo = _repo(tmp_path)
    monkeypatch.chdir(repo)

    assert cli.main(["disable", "alpha"]) == 0
    assert load_manifest(repo).skills["alpha"].enabled is False
    assert "enabled = false" in (repo / "my-skills.toml").read_text()

    assert cli.main(["enable", "alpha"]) == 0
    assert load_manifest(repo).skills["alpha"].enabled is True
    assert "enabled = true" in (repo / "my-skills.toml").read_text()


def test_enable_unknown_skill_blocks(tmp_path, monkeypatch, capsys):
    repo = _repo(tmp_path)
    monkeypatch.chdir(repo)

    assert cli.main(["enable", "missing"]) == 2
    assert "unknown skill" in capsys.readouterr().err


def test_register_skill_appends_new_simple_section(tmp_path):
    repo = _repo(tmp_path)

    register_skill(
        repo / "my-skills.toml",
        "brand",
        enabled=False,
        hosts=["claude", "codex"],
    )

    manifest = load_manifest(repo)
    assert manifest.skills["brand"].enabled is False
    assert manifest.skills["brand"].hosts == ["claude", "codex"]
    text = (repo / "my-skills.toml").read_text()
    assert "[skills.brand]" in text
    assert 'hosts = ["claude", "codex"]' in text


def test_register_skill_rejects_invalid_name(tmp_path):
    repo = _repo(tmp_path)

    with pytest.raises(ManifestEditError, match="invalid skill name"):
        register_skill(repo / "my-skills.toml", "Bad_Name", enabled=True, hosts=["claude"])
