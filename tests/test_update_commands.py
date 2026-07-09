from __future__ import annotations

import subprocess
from collections.abc import Sequence

import my_skills.update_commands as update_commands
from my_skills import cli


def _completed(
    command: Sequence[str], stdout: str = "", stderr: str = "", returncode: int = 0
) -> subprocess.CompletedProcess[str]:
    return subprocess.CompletedProcess(list(command), returncode, stdout, stderr)


def _install_info(version: str = "0.2.0") -> update_commands.InstallInfo:
    return update_commands.InstallInfo(
        version=version,
        source_url=None,
        requested_revision=None,
        commit_id=None,
        executable="/bin/my-skills",
    )


def test_latest_stable_ref_uses_highest_semver_tag_and_ignores_prerelease() -> None:
    def run(
        command: Sequence[str], timeout: int | None = None
    ) -> subprocess.CompletedProcess[str]:
        return _completed(
            command,
            stdout=(
                "1111111111111111111111111111111111111111\trefs/tags/v0.2.0\n"
                "2222222222222222222222222222222222222222\trefs/tags/v0.10.0\n"
                "3333333333333333333333333333333333333333\trefs/tags/v0.3.1\n"
                "4444444444444444444444444444444444444444\trefs/tags/v0.11.0-rc.1\n"
                "5555555555555555555555555555555555555555\trefs/tags/not-a-release\n"
            ),
        )

    ref = update_commands.latest_stable_ref(run=run, which=lambda name: "/usr/bin/git")

    assert ref.name == "v0.10.0"
    assert ref.version == (0, 10, 0)
    assert ref.commitish == "2222222222222222222222222222222222222222"


def test_format_update_status_reports_available_stable_release() -> None:
    status = update_commands.check_update(
        install_info_reader=lambda: _install_info("0.2.0"),
        stable_ref_reader=lambda: update_commands.RemoteRef(
            name="v0.3.0",
            version=(0, 3, 0),
            commitish="abc123",
        ),
    )

    assert status.state == "available"
    assert update_commands.format_update_status(status) == (
        "Update:  available v0.3.0 (run 'my-skills update')"
    )


def test_update_check_exits_one_when_stable_update_is_available(
    monkeypatch, capsys
) -> None:
    monkeypatch.setattr(update_commands, "read_install_info", lambda: _install_info("0.2.0"))
    monkeypatch.setattr(
        update_commands,
        "latest_stable_ref",
        lambda: update_commands.RemoteRef(
            name="v0.3.0",
            version=(0, 3, 0),
            commitish="abc123",
        ),
    )

    rc = cli.main(["update", "--check"])

    out = capsys.readouterr().out
    assert rc == 1
    assert "Current: my-skills 0.2.0" in out
    assert "Latest:  v0.3.0" in out
    assert "Update available" in out


def test_update_dry_run_prints_main_channel_install_command(monkeypatch, capsys) -> None:
    monkeypatch.setattr(update_commands, "read_install_info", lambda: _install_info("0.2.0"))
    monkeypatch.setattr(
        update_commands,
        "latest_main_ref",
        lambda: update_commands.RemoteRef(
            name="main",
            version=None,
            commitish="abcdef1234567890",
        ),
    )

    rc = cli.main(["update", "--channel", "main", "--dry-run"])

    out = capsys.readouterr().out
    assert rc == 0
    assert "Current: my-skills 0.2.0" in out
    assert "Target:  main (abcdef1)" in out
    assert (
        "uv tool install --force git+https://github.com/Seosiju/my-skills.git@main"
        in out
    )


def test_update_reports_missing_uv_without_installing(monkeypatch, capsys) -> None:
    monkeypatch.setattr(update_commands, "read_install_info", lambda: _install_info("0.2.0"))
    monkeypatch.setattr(
        update_commands,
        "latest_stable_ref",
        lambda: update_commands.RemoteRef(
            name="v0.3.0",
            version=(0, 3, 0),
            commitish="abc123",
        ),
    )
    monkeypatch.setattr(update_commands.shutil, "which", lambda name: None)

    rc = cli.main(["update"])

    err = capsys.readouterr().err
    assert rc == 2
    assert "uv not found; install uv first" in err


def test_update_runs_uv_install_and_verifies_stable_version(monkeypatch, capsys) -> None:
    calls: list[list[str]] = []

    def run(
        command: Sequence[str], timeout: int | None = None
    ) -> subprocess.CompletedProcess[str]:
        calls.append(list(command))
        if command[0] == "/bin/uv":
            return _completed(command)
        return _completed(command, stdout="my-skills 0.3.0\n")

    monkeypatch.setattr(update_commands, "read_install_info", lambda: _install_info("0.2.0"))
    monkeypatch.setattr(
        update_commands,
        "latest_stable_ref",
        lambda: update_commands.RemoteRef(
            name="v0.3.0",
            version=(0, 3, 0),
            commitish="abc123",
        ),
    )
    monkeypatch.setattr(
        update_commands.shutil,
        "which",
        lambda name: "/bin/uv" if name == "uv" else "/bin/my-skills",
    )
    monkeypatch.setattr(update_commands, "run_command", run)

    rc = cli.main(["update"])

    out = capsys.readouterr().out
    assert rc == 0
    assert calls[0] == [
        "/bin/uv",
        "tool",
        "install",
        "--force",
        "git+https://github.com/Seosiju/my-skills.git@v0.3.0",
    ]
    assert calls[1] == ["/bin/my-skills", "--version"]
    assert "Updated: my-skills 0.3.0" in out
