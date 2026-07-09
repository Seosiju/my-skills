from __future__ import annotations

import shutil
import subprocess
from collections.abc import Sequence

import pytest

import my_skills.update_commands as update_commands
from my_skills import cli


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


def _uv_or_my_skills(name: str) -> str:
    return "/bin/uv" if name == "uv" else "/bin/my-skills"


def _main_ref() -> update_commands.RemoteRef:
    return update_commands.RemoteRef(
        name="main",
        version=None,
        commitish="abcdef1234567890",
    )


def test_main_channel_current_when_commit_matches() -> None:
    status = update_commands.check_update(
        "main",
        install_info_reader=lambda: _install_info("0.2.0", commit_id="abcdef"),
        main_ref_reader=lambda: update_commands.RemoteRef(
            name="main",
            version=None,
            commitish="abcdef",
        ),
    )

    assert status.state == "current"


def test_main_update_installs_main_and_verifies_commit_metadata(
    monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    calls: list[list[str]] = []
    installs = iter(
        [
            _install_info("0.2.0", commit_id="old"),
            _install_info("0.2.0", commit_id="abcdef1234567890"),
        ]
    )

    def run(
        command: Sequence[str], _timeout: int | None = None
    ) -> subprocess.CompletedProcess[str]:
        calls.append(list(command))
        if command[0] == "/bin/uv":
            return _completed(command)
        return _completed(command, stdout="my-skills 0.2.0\n")

    monkeypatch.setattr(update_commands, "read_install_info", lambda: next(installs))
    monkeypatch.setattr(update_commands, "latest_main_ref", _main_ref)
    monkeypatch.setattr(shutil, "which", _uv_or_my_skills)
    monkeypatch.setattr(update_commands, "run_command", run)

    rc = cli.main(["update", "--channel", "main"])

    captured = capsys.readouterr()
    assert rc == 0
    assert calls[0] == [
        "/bin/uv",
        "tool",
        "install",
        "--force",
        "git+https://github.com/Seosiju/my-skills.git@main",
    ]
    assert calls[1] == ["/bin/my-skills", "--version"]
    assert "Updated: my-skills 0.2.0 from main" in captured.out
    assert "warning:" not in captured.err


def test_main_update_warns_when_commit_metadata_is_unavailable(
    monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    installs = iter([_install_info("0.2.0", commit_id="old"), _install_info("0.2.0")])

    def run(
        command: Sequence[str], _timeout: int | None = None
    ) -> subprocess.CompletedProcess[str]:
        if command[0] == "/bin/uv":
            return _completed(command)
        return _completed(command, stdout="my-skills 0.2.0\n")

    monkeypatch.setattr(update_commands, "read_install_info", lambda: next(installs))
    monkeypatch.setattr(update_commands, "latest_main_ref", _main_ref)
    monkeypatch.setattr(shutil, "which", _uv_or_my_skills)
    monkeypatch.setattr(update_commands, "run_command", run)

    rc = cli.main(["update", "--channel", "main"])

    captured = capsys.readouterr()
    assert rc == 0
    assert "Updated: my-skills 0.2.0 from main" in captured.out
    assert "warning: updated main commit could not be verified" in captured.err


def test_main_update_fails_when_commit_metadata_mismatches(
    monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    installs = iter(
        [
            _install_info("0.2.0", commit_id="old"),
            _install_info("0.2.0", commit_id="different"),
        ]
    )

    def run(
        command: Sequence[str], _timeout: int | None = None
    ) -> subprocess.CompletedProcess[str]:
        if command[0] == "/bin/uv":
            return _completed(command)
        return _completed(command, stdout="my-skills 0.2.0\n")

    monkeypatch.setattr(update_commands, "read_install_info", lambda: next(installs))
    monkeypatch.setattr(update_commands, "latest_main_ref", _main_ref)
    monkeypatch.setattr(shutil, "which", _uv_or_my_skills)
    monkeypatch.setattr(update_commands, "run_command", run)

    rc = cli.main(["update", "--channel", "main"])

    captured = capsys.readouterr()
    assert rc == 1
    assert "updated main commit mismatch" in captured.err
