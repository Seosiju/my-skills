from __future__ import annotations

import pytest

from my_skills import cli


def _make_registry(path):
    path.mkdir(parents=True)
    (path / "my-skills.toml").write_text(
        'schema_version = 1\n'
        'skills_root = "skills"\n'
        "\n"
        "[skills.example]\n"
        "enabled = true\n"
        'hosts = ["codex"]\n'
        "\n"
        "[skills.disabled]\n"
        "enabled = false\n"
        'hosts = ["codex"]\n',
        encoding="utf-8",
    )
    (path / "skills").mkdir()
    return path


def test_version_flag_prints_cli_version(capsys):
    with pytest.raises(SystemExit) as exc:
        cli.main(["--version"])

    assert exc.value.code == 0
    assert capsys.readouterr().out.strip() == "my-skills 0.1.0"


def test_doctor_reports_registry_source_version_state_and_data(
    tmp_path, monkeypatch, capsys
):
    registry = _make_registry(tmp_path / "registry")
    assert cli.main(["set-root", str(registry)]) == 0
    capsys.readouterr()
    (tmp_path / "outside").mkdir(exist_ok=True)
    monkeypatch.chdir(tmp_path / "outside")

    rc = cli.main(["doctor"])

    out = capsys.readouterr().out
    assert rc == 0
    assert "my-skills 0.1.0" in out
    assert f"Registry: {registry.resolve()} (source: cache)" in out
    assert "Skills:   2 registered, 1 enabled" in out
    assert f"State:    {tmp_path / 'xdg-state' / 'my-skills' / 'state.json'}" in out
    assert f"Data:     {tmp_path / 'xdg-data' / 'my-skills'}" in out
    assert "Manifest: valid" in out


def test_doctor_without_configured_root_is_diagnostic_success(
    tmp_path, monkeypatch, capsys
):
    monkeypatch.chdir(tmp_path)

    rc = cli.main(["doctor"])

    out = capsys.readouterr().out
    assert rc == 0
    assert "Registry: not configured" in out
    assert "init-registry" in out
    assert "set-root" in out


def test_doctor_reports_invalid_env_root_as_manifest_error(
    tmp_path, monkeypatch, capsys
):
    monkeypatch.chdir(tmp_path)
    monkeypatch.setenv("MY_SKILLS_ROOT", str(tmp_path / "missing"))

    rc = cli.main(["doctor"])

    out = capsys.readouterr().out
    assert rc == 1
    assert "Registry: INVALID" in out
    assert "MY_SKILLS_ROOT" in out
    assert "Manifest: INVALID" in out
