from __future__ import annotations

import importlib.metadata as importlib_metadata
import shutil
import subprocess
from collections.abc import Sequence
from typing import final

import pytest

import my_skills.update_commands as update_commands
from my_skills import cli


@final
class _Distribution:
    def __init__(self, direct_url: str | None) -> None:
        self._direct_url = direct_url

    def read_text(self, filename: str) -> str | None:
        if filename == "direct_url.json":
            return self._direct_url
        return None


def _completed(
    command: Sequence[str], stdout: str = "", stderr: str = "", returncode: int = 0
) -> subprocess.CompletedProcess[str]:
    return subprocess.CompletedProcess(list(command), returncode, stdout, stderr)


def _install_info(version: str, commit_id: str | None = None) -> update_commands.InstallInfo:
    return update_commands.InstallInfo(
        version=version,
        source_url="https://github.com/Seosiju/my-skills.git",
        requested_revision="main" if commit_id else None,
        commit_id=commit_id,
        executable="/bin/my-skills",
    )


def _stable_ref(version: tuple[int, int, int]) -> update_commands.RemoteRef:
    return update_commands.RemoteRef(
        name=f"v{version[0]}.{version[1]}.{version[2]}",
        version=version,
        commitish="abc123",
    )


def _git_path(_name: str) -> str:
    return "/usr/bin/git"


def _missing_command(_name: str) -> None:
    return None


def _my_skills_path(_name: str) -> str:
    return "/bin/my-skills"


def _uv_or_my_skills(name: str) -> str:
    return "/bin/uv" if name == "uv" else "/bin/my-skills"


def _version_020(_name: str) -> str:
    return "0.2.0"


def test_latest_stable_ref_reports_missing_git() -> None:
    try:
        _ = update_commands.latest_stable_ref(which=_missing_command)
    except update_commands.UpdateCheckError as exc:
        assert str(exc) == "git not found"
    else:
        raise AssertionError("missing git should raise UpdateCheckError")


def test_latest_stable_ref_reports_when_no_stable_tags_exist() -> None:
    def run(
        command: Sequence[str], _timeout: int | None = None
    ) -> subprocess.CompletedProcess[str]:
        return _completed(command, stdout="111\trefs/tags/v0.3.0-rc.1\n")

    try:
        _ = update_commands.latest_stable_ref(run=run, which=_git_path)
    except update_commands.UpdateCheckError as exc:
        assert str(exc) == "no stable release tags found"
    else:
        raise AssertionError("missing stable tags should raise UpdateCheckError")


def test_read_install_info_tolerates_broken_direct_url(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    def distribution(_name: str) -> _Distribution:
        return _Distribution("{not json")

    monkeypatch.setattr(importlib_metadata, "version", _version_020)
    monkeypatch.setattr(
        importlib_metadata,
        "distribution",
        distribution,
    )
    monkeypatch.setattr(shutil, "which", _my_skills_path)

    info = update_commands.read_install_info()

    assert info.version == "0.2.0"
    assert info.source_url is None
    assert info.requested_revision is None
    assert info.commit_id is None
    assert info.executable == "/bin/my-skills"


def test_read_install_info_rejects_malformed_direct_url_with_known_keys(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    direct_url = (
        '{"url":"https://github.com/Seosiju/my-skills.git",'
        '"vcs_info":{"requested_revision":"main","commit_id":"abcdef"},'
    )

    def distribution(_name: str) -> _Distribution:
        return _Distribution(direct_url)

    monkeypatch.setattr(importlib_metadata, "version", _version_020)
    monkeypatch.setattr(importlib_metadata, "distribution", distribution)
    monkeypatch.setattr(shutil, "which", _my_skills_path)

    info = update_commands.read_install_info()

    assert info.source_url is None
    assert info.requested_revision is None
    assert info.commit_id is None


def test_read_install_info_reads_direct_url_vcs_metadata(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    direct_url = (
        '{"url":"https://github.com/Seosiju/my-skills.git",'
        '"vcs_info":{"requested_revision":"main","commit_id":"abcdef"}}'
    )

    def distribution(_name: str) -> _Distribution:
        return _Distribution(direct_url)

    monkeypatch.setattr(importlib_metadata, "version", _version_020)
    monkeypatch.setattr(
        importlib_metadata,
        "distribution",
        distribution,
    )
    monkeypatch.setattr(shutil, "which", _my_skills_path)

    info = update_commands.read_install_info()

    assert info.source_url == "https://github.com/Seosiju/my-skills.git"
    assert info.requested_revision == "main"
    assert info.commit_id == "abcdef"


def test_stable_status_current_ahead_and_unknown_current() -> None:
    latest = _stable_ref((0, 3, 0))

    current = update_commands.check_update(
        install_info_reader=lambda: _install_info("0.3.0"),
        stable_ref_reader=lambda: latest,
    )
    ahead = update_commands.check_update(
        install_info_reader=lambda: _install_info("0.4.0"),
        stable_ref_reader=lambda: latest,
    )
    unknown = update_commands.check_update(
        install_info_reader=lambda: _install_info("dev"),
        stable_ref_reader=lambda: latest,
    )

    assert current.state == "current"
    assert ahead.state == "ahead"
    assert unknown.state == "unknown-current"


def test_update_returns_two_when_stable_check_cannot_resolve_tags(
    monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    monkeypatch.setattr(update_commands, "read_install_info", lambda: _install_info("0.2.0"))

    def fail_latest() -> update_commands.RemoteRef:
        raise update_commands.UpdateCheckError("git not found")

    monkeypatch.setattr(update_commands, "latest_stable_ref", fail_latest)

    rc = cli.main(["update", "--check"])

    captured = capsys.readouterr()
    assert rc == 2
    assert "Update status unknown: git not found" in captured.err


def test_post_install_version_mismatch_fails_stable_update(
    monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    def run(
        command: Sequence[str], _timeout: int | None = None
    ) -> subprocess.CompletedProcess[str]:
        if command[0] == "/bin/uv":
            return _completed(command)
        return _completed(command, stdout="my-skills 0.2.0\n")

    monkeypatch.setattr(update_commands, "read_install_info", lambda: _install_info("0.2.0"))
    monkeypatch.setattr(
        update_commands,
        "latest_stable_ref",
        lambda: _stable_ref((0, 3, 0)),
    )
    monkeypatch.setattr(shutil, "which", _uv_or_my_skills)
    monkeypatch.setattr(update_commands, "run_command", run)

    rc = cli.main(["update"])

    assert rc == 1
    assert "updated command did not report expected version 0.3.0" in capsys.readouterr().err
