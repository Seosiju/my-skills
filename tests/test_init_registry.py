from __future__ import annotations

import json
import tomllib
from pathlib import Path

import pytest

from my_skills import cli
from my_skills.defaults import DEFAULT_SEED_SKILLS


SEED_HOSTS = ["claude", "codex", "hermes"]


def _init_registry(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> Path:
    home = tmp_path / "home"
    home.mkdir()
    monkeypatch.setenv("HOME", str(home))
    monkeypatch.setenv("XDG_STATE_HOME", str(home / ".local" / "state"))

    target = tmp_path / "my-agent-skills"
    assert cli.main(["init-registry", str(target)]) == 0
    return target


def test_init_registry_registers_seed_skills_in_manifest(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    target = _init_registry(tmp_path, monkeypatch)
    capsys.readouterr()

    manifest = tomllib.loads((target / "my-skills.toml").read_text(encoding="utf-8"))

    assert set(manifest["skills"]) == {name for name, _enabled in DEFAULT_SEED_SKILLS}
    for name, enabled in DEFAULT_SEED_SKILLS:
        assert manifest["skills"][name]["enabled"] is enabled
        assert manifest["skills"][name]["hosts"] == SEED_HOSTS


def test_init_registry_install_dry_run_has_seed_actions(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    target = _init_registry(tmp_path, monkeypatch)
    monkeypatch.chdir(target)
    capsys.readouterr()

    rc = cli.main(["install", "--dry-run", "--json"])

    payload = json.loads(capsys.readouterr().out)
    assert rc == 0
    assert payload["actions"]
    action_skills = {action["skill"] for action in payload["actions"]}
    assert {"cli-inventory", "personal-profile", "my-skills"} <= action_skills
    assert "my-jira" not in action_skills


def test_init_registry_no_defaults_has_no_skill_manifest_entries(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    home = tmp_path / "home"
    home.mkdir()
    monkeypatch.setenv("HOME", str(home))

    target = tmp_path / "my-agent-skills"
    assert cli.main(["init-registry", str(target), "--no-defaults"]) == 0
    capsys.readouterr()

    manifest = tomllib.loads((target / "my-skills.toml").read_text(encoding="utf-8"))
    assert "skills" not in manifest
