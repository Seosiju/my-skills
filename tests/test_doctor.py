from __future__ import annotations

from pathlib import Path

import pytest

import my_skills.update_commands as update_commands
from my_skills import __version__, cli


def _make_registry(path: Path) -> Path:
    path.mkdir(parents=True)
    manifest = "\n".join(
        [
            "schema_version = 1",
            'skills_root = "skills"',
            "",
            "[skills.example]",
            "enabled = true",
            'hosts = ["codex"]',
            "",
            "[skills.disabled]",
            "enabled = false",
            'hosts = ["codex"]',
            "",
        ]
    )
    _ = (path / "my-skills.toml").write_text(manifest, encoding="utf-8")
    (path / "skills").mkdir()
    return path


def test_version_flag_prints_cli_version(capsys: pytest.CaptureFixture[str]) -> None:
    with pytest.raises(SystemExit) as exc:
        _ = cli.main(["--version"])

    assert exc.value.code == 0
    assert capsys.readouterr().out.strip() == f"my-skills {__version__}"


def test_doctor_reports_registry_source_version_state_and_data(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
) -> None:
    registry = _make_registry(tmp_path / "registry")
    assert cli.main(["set-root", str(registry)]) == 0
    _ = capsys.readouterr()
    (tmp_path / "outside").mkdir(exist_ok=True)
    monkeypatch.chdir(tmp_path / "outside")
    monkeypatch.setattr(
        update_commands,
        "format_doctor_update_status",
        lambda: "Update:  up to date (stable v0.2.0)",
    )

    rc = cli.main(["doctor"])

    out = capsys.readouterr().out
    assert rc == 0
    assert f"my-skills {__version__}" in out
    assert "Update:  up to date (stable v0.2.0)" in out
    assert f"Registry: {registry.resolve()} (source: cache)" in out
    assert "Skills:   2 registered, 1 enabled" in out
    assert f"State:    {tmp_path / 'xdg-state' / 'my-skills' / 'state.json'}" in out
    assert f"Data:     {tmp_path / 'xdg-data' / 'my-skills'}" in out
    assert "Manifest: valid" in out


def test_doctor_without_configured_root_is_diagnostic_success(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
) -> None:
    monkeypatch.chdir(tmp_path)
    monkeypatch.setattr(
        update_commands,
        "format_doctor_update_status",
        lambda: "Update:  not checked (network unavailable)",
    )

    rc = cli.main(["doctor"])

    out = capsys.readouterr().out
    assert rc == 0
    assert "Update:  not checked (network unavailable)" in out
    assert "Registry: not configured" in out
    assert "init-registry" in out
    assert "set-root" in out


def test_doctor_reports_invalid_env_root_as_manifest_error(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
) -> None:
    monkeypatch.chdir(tmp_path)
    monkeypatch.setenv("MY_SKILLS_ROOT", str(tmp_path / "missing"))
    monkeypatch.setattr(
        update_commands,
        "format_doctor_update_status",
        lambda: "Update:  up to date (stable v0.2.0)",
    )

    rc = cli.main(["doctor"])

    out = capsys.readouterr().out
    assert rc == 1
    assert "Registry: INVALID" in out
    assert "MY_SKILLS_ROOT" in out
    assert "Manifest: INVALID" in out


def test_doctor_no_update_check_skips_remote_lookup(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
) -> None:
    registry = _make_registry(tmp_path / "registry")
    assert cli.main(["set-root", str(registry)]) == 0
    _ = capsys.readouterr()
    (tmp_path / "outside").mkdir(exist_ok=True)
    monkeypatch.chdir(tmp_path / "outside")

    def fail_update_check() -> str:
        raise AssertionError("doctor --no-update-check should not query update state")

    monkeypatch.setattr(update_commands, "format_doctor_update_status", fail_update_check)

    rc = cli.main(["doctor", "--no-update-check"])

    out = capsys.readouterr().out
    assert rc == 0
    assert f"my-skills {__version__}" in out
    assert "Update:  skipped" in out
    assert f"Registry: {registry.resolve()} (source: cache)" in out
    assert "Manifest: valid" in out
