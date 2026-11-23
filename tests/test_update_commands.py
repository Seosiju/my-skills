from __future__ import annotations

import shutil
import subprocess
from collections.abc import Sequence

import pytest

import my_skills.update_commands as update_commands
from my_skills import cli

# The autouse fixture in conftest.py replaces this lookup; keep the real one.
_REAL_LATEST_RUST_REF = update_commands.latest_rust_ref


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


def _git_path(_name: str) -> str:
    return "/usr/bin/git"


def test_latest_stable_ref_uses_highest_semver_tag_and_ignores_prerelease() -> None:
    def run(
        command: Sequence[str], _timeout: int | None = None
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

    ref = update_commands.latest_stable_ref(run=run, which=_git_path)

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
    monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
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


def test_update_dry_run_prints_main_channel_install_command(
    monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
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


def test_update_reports_missing_uv_without_installing(
    monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
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

    def missing_command(_name: str) -> None:
        return None

    monkeypatch.setattr(shutil, "which", missing_command)

    rc = cli.main(["update"])

    err = capsys.readouterr().err
    assert rc == 2
    assert "uv not found; install uv first" in err


def test_update_runs_uv_install_and_verifies_stable_version(
    monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    calls: list[list[str]] = []

    def run(
        command: Sequence[str], _timeout: int | None = None
    ) -> subprocess.CompletedProcess[str]:
        calls.append(list(command))
        if command[0] == "/bin/uv":
            return _completed(command)
        return _completed(command, stdout="my-skills 0.3.0\n")

    def find_command(name: str) -> str:
        return "/bin/uv" if name == "uv" else "/bin/my-skills"

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
    monkeypatch.setattr(shutil, "which", find_command)
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


def _mock_stable_cutover(
    monkeypatch: pytest.MonkeyPatch, current: str, tags: tuple[str, ...]
) -> None:
    latest_stable_ref = update_commands.latest_stable_ref

    def run(
        command: Sequence[str], _timeout: int | None = None
    ) -> subprocess.CompletedProcess[str]:
        return _completed(
            command, stdout="".join(f"abc123\trefs/tags/{tag}\n" for tag in tags)
        )

    def unexpected(*_args: object) -> None:
        pytest.fail("migration must not look up uv or run the installer")

    monkeypatch.setattr(update_commands, "read_install_info", lambda: _install_info(current))
    monkeypatch.setattr(
        update_commands, "latest_stable_ref", lambda: latest_stable_ref(run=run, which=_git_path)
    )
    monkeypatch.setattr(update_commands, "run_command", unexpected)
    monkeypatch.setattr(shutil, "which", unexpected)


@pytest.mark.parametrize("flags", [[], ["--dry-run"]])
@pytest.mark.parametrize("release", ["v1.0.0", "v2.0.0"])
def test_update_directs_python_users_to_binary_installers(
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
    flags: list[str],
    release: str,
) -> None:
    _mock_stable_cutover(monkeypatch, "0.4.2", ("v0.4.2", release))

    assert cli.main(["update", *flags]) == 0

    captured = capsys.readouterr()
    assert captured.out.splitlines() == [
        "Current: my-skills 0.4.2",
        f"my-skills {release} is available. Install the Rust binary with:",
        f"  macOS/Linux: https://github.com/Seosiju/my-skills/blob/{release}/scripts/install.sh",
        f"  Windows:     https://github.com/Seosiju/my-skills/blob/{release}/scripts/install.ps1",
        f"  Migration spec: https://github.com/Seosiju/my-skills/blob/{release}/"
        "docs/2026-11-23-skill-drive-rust-v1-spec.md",
    ]
    assert captured.err == ""


@pytest.mark.parametrize(
    ("current", "code", "status"),
    [("0.4.1", 1, "Update available"), ("0.4.2", 0, "Already up to date")],
)
def test_update_check_reports_compatible_python_release_before_migration_notice(
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
    current: str,
    code: int,
    status: str,
) -> None:
    _mock_stable_cutover(
        monkeypatch, current, ("v0.4.1", "v0.4.2", "v0.4.3-rc.1", "v0.5.0", "v1.0.0")
    )

    assert cli.main(["update", "--check"]) == code

    captured = capsys.readouterr()
    assert captured.out.splitlines() == [
        f"Current: my-skills {current}",
        "Latest:  v0.4.2",
        status,
        "my-skills v1.0.0 is available.",
    ]
    assert captured.err == ""


def test_update_check_does_not_suggest_migration_for_prerelease_only(
    monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    _mock_stable_cutover(monkeypatch, "0.4.2", ("v0.4.2", "v1.0.0-rc.1"))

    assert cli.main(["update", "--check"]) == 0
    assert capsys.readouterr().out.splitlines() == [
        "Current: my-skills 0.4.2",
        "Latest:  v0.4.2",
        "Already up to date",
    ]


def _tags(*tags: str):
    def run(command: Sequence[str], _timeout: int | None = None) -> subprocess.CompletedProcess[str]:
        return _completed(command, stdout="".join(f"abc123\trefs/tags/{tag}\n" for tag in tags))

    return run


@pytest.mark.parametrize(
    ("tags", "expected"),
    [
        (("v0.4.1", "v0.4.2"), None),
        (("v0.4.2", "v1.0.0-alpha.1", "v1.0.0-alpha.2"), "v1.0.0-alpha.2"),
        (("v1.0.0-alpha.2", "v1.0.0", "v1.0.0-rc.1"), "v1.0.0"),
        (("v1.0.0", "v1.1.0-alpha.1"), "v1.1.0-alpha.1"),
    ],
)
def test_latest_rust_ref_includes_prereleases_and_prefers_stable(
    tags: tuple[str, ...], expected: str | None
) -> None:
    ref = _REAL_LATEST_RUST_REF(run=_tags(*tags), which=_git_path)
    assert (ref.name if ref else None) == expected


def _mock_main_cutover(monkeypatch: pytest.MonkeyPatch, release: str) -> None:
    def unexpected(*_args: object) -> None:
        pytest.fail("main-channel migration must not read main or run the installer")

    monkeypatch.setattr(update_commands, "read_install_info", lambda: _install_info("0.4.2"))
    monkeypatch.setattr(
        update_commands,
        "latest_rust_ref",
        lambda: _REAL_LATEST_RUST_REF(run=_tags("v0.4.2", release), which=_git_path),
    )
    monkeypatch.setattr(update_commands, "latest_main_ref", unexpected)
    monkeypatch.setattr(update_commands, "run_command", unexpected)
    monkeypatch.setattr(shutil, "which", unexpected)


@pytest.mark.parametrize("flags", [[], ["--dry-run"]])
def test_main_channel_directs_to_rust_binary_once_any_v1_tag_exists(
    monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str], flags: list[str]
) -> None:
    _mock_main_cutover(monkeypatch, "v1.0.0-alpha.1")

    assert cli.main(["update", "--channel", "main", *flags]) == 0

    captured = capsys.readouterr()
    base = "https://github.com/Seosiju/my-skills/blob/v1.0.0-alpha.1"
    assert captured.out.splitlines() == [
        "Current: my-skills 0.4.2",
        "my-skills v1.0.0-alpha.1 is available. Install the Rust binary with:",
        f"  macOS/Linux: {base}/scripts/install.sh",
        f"  Windows:     {base}/scripts/install.ps1",
        f"  Migration spec: {base}/docs/2026-11-23-skill-drive-rust-v1-spec.md",
    ]
    assert captured.err == ""


def test_main_channel_check_reports_migration_without_failing(
    monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    _mock_main_cutover(monkeypatch, "v1.0.0-alpha.1")

    assert cli.main(["update", "--channel", "main", "--check"]) == 0
    assert capsys.readouterr().out.splitlines() == [
        "Current: my-skills 0.4.2",
        "Python updates have moved to the Rust v1 binary",
        "my-skills v1.0.0-alpha.1 is available.",
    ]


def test_doctor_update_line_mentions_rust_release() -> None:
    info = _install_info("0.4.2")
    v042 = update_commands.RemoteRef(name="v0.4.2", version=(0, 4, 2), commitish="a")
    v1 = update_commands.RemoteRef(name="v1.0.0", version=(1, 0, 0), commitish="b")
    stable = update_commands.UpdateStatus(info, "stable", v042, "current", "v0.4.2", migration=v1)
    assert update_commands.format_update_status(stable) == (
        "Update:  up to date (stable v0.4.2); Rust v1.0.0 is available "
        "(run 'my-skills update' for install steps)"
    )
    main = update_commands.UpdateStatus(info, "main", None, "migrate", "moved", migration=v1)
    assert update_commands.format_update_status(main) == (
        "Update:  main channel moved to the Rust binary v1.0.0 "
        "(run 'my-skills update' for install steps)"
    )
