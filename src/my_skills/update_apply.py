from __future__ import annotations

import argparse
import shutil
import subprocess
import sys
from collections.abc import Callable, Sequence
from typing import assert_never

from . import update_commands

VersionRunner = Callable[
    [Sequence[str], int | None],
    subprocess.CompletedProcess[str],
]


def _install_target(status: update_commands.UpdateStatus) -> str:
    if status.latest is None:
        raise update_commands.UpdateCheckError("latest ref unavailable")
    match status.channel:
        case "stable":
            return status.latest.name
        case "main":
            return "main"
        case unreachable:
            assert_never(unreachable)


def _display_command(command: Sequence[str]) -> str:
    display = ["uv", *command[1:]] if command and command[0] != "uv" else list(command)
    return " ".join(display)


def _install_command(uv: str, target: str) -> list[str]:
    return [
        uv,
        "tool",
        "install",
        "--force",
        f"git+{update_commands.REPO_URL}@{target}",
    ]


def _print_target(status: update_commands.UpdateStatus) -> None:
    if status.latest is None:
        return
    match status.channel:
        case "stable":
            print(f"Latest:  {status.latest.name}")
        case "main":
            print(f"Target:  main ({status.latest.commitish[:7]})")


def _check_exit_code(status: update_commands.UpdateStatus) -> int:
    match status.state:
        case "available":
            print("Update available")
            return 1
        case "not-checked" | "unknown-current":
            print(f"Update status unknown: {status.detail}", file=sys.stderr)
            return 2
        case "current":
            print("Already up to date")
            return 0
        case "ahead":
            print("Installed version is ahead of the latest stable release")
            return 0
        case unreachable:
            assert_never(unreachable)


def _verified_version(
    status: update_commands.UpdateStatus, run: VersionRunner | None = None
) -> str | None:
    runner = update_commands._run_command if run is None else run
    executable = shutil.which("my-skills") or "my-skills"
    try:
        result = runner([executable, "--version"], update_commands.DEFAULT_TIMEOUT_SECONDS)
    except FileNotFoundError:
        print("error: my-skills executable not found after update", file=sys.stderr)
        return None
    except subprocess.TimeoutExpired:
        print("error: updated my-skills --version timed out", file=sys.stderr)
        return None

    observed = result.stdout.strip()
    if result.returncode != 0 or not observed:
        print("error: updated my-skills --version failed", file=sys.stderr)
        return None
    if status.channel == "stable" and status.latest and status.latest.version is not None:
        expected = ".".join(str(part) for part in status.latest.version)
        if observed != f"my-skills {expected}":
            print(
                "error: updated command did not report expected version "
                f"{expected}: {observed}",
                file=sys.stderr,
            )
            return None
    return observed


def _verify_main_commit(status: update_commands.UpdateStatus) -> bool:
    if status.channel != "main" or status.latest is None:
        return True
    installed = update_commands.read_install_info()
    if installed.commit_id is None:
        print(
            "warning: updated main commit could not be verified "
            "(install metadata unavailable)",
            file=sys.stderr,
        )
        return True
    if installed.commit_id != status.latest.commitish:
        print(
            "error: updated main commit mismatch: "
            f"expected {status.latest.commitish[:7]}, got {installed.commit_id[:7]}",
            file=sys.stderr,
        )
        return False
    return True


def cmd_update(args: argparse.Namespace) -> int:
    status = update_commands.check_update(args.channel)
    print(f"Current: my-skills {status.current.version}")
    _print_target(status)

    if args.check:
        return _check_exit_code(status)
    if status.state == "not-checked" or status.latest is None:
        print(f"error: could not resolve update target ({status.detail})", file=sys.stderr)
        return 2
    if status.state == "current":
        print("Already up to date")
        return 0
    if status.state == "ahead":
        print("Installed version is ahead of the latest stable release")
        return 0

    try:
        target = _install_target(status)
    except update_commands.UpdateCheckError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2

    preview = _install_command("uv", target)
    if args.dry_run:
        print(f"Would run: {_display_command(preview)}")
        return 0

    uv = shutil.which("uv")
    if uv is None:
        print("error: uv not found; install uv first", file=sys.stderr)
        return 2

    command = _install_command(uv, target)
    print(f"Updating via {_display_command(command)}")
    try:
        result = update_commands._run_command(
            command, update_commands.INSTALL_TIMEOUT_SECONDS
        )
    except FileNotFoundError:
        print("error: uv not found; install uv first", file=sys.stderr)
        return 2
    except subprocess.TimeoutExpired:
        print("error: uv tool install timed out", file=sys.stderr)
        return 1

    if result.returncode != 0:
        print(
            f"error: uv tool install failed: {update_commands._first_stderr_line(result)}",
            file=sys.stderr,
        )
        return 1

    observed = _verified_version(status)
    if observed is None:
        return 1
    if not _verify_main_commit(status):
        return 1
    suffix = " from main" if status.channel == "main" else ""
    print(f"Updated: {observed}{suffix}")
    return 0
