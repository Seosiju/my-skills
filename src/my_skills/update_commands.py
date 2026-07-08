from __future__ import annotations

import json
import re
import shutil
import subprocess
from collections.abc import Callable, Sequence
from dataclasses import dataclass
from importlib import metadata
from typing import Literal

from . import __version__

REPO_URL = "https://github.com/Seosiju/my-skills.git"
SEMVER_TAG_RE = re.compile(r"^refs/tags/v(\d+)\.(\d+)\.(\d+)$")
VERSION_RE = re.compile(r"^v?(\d+)\.(\d+)\.(\d+)$")
DEFAULT_TIMEOUT_SECONDS = 3
INSTALL_TIMEOUT_SECONDS = 300

UpdateChannel = Literal["stable", "main"]
UpdateState = Literal["available", "current", "ahead", "not-checked", "unknown-current"]
CommandRunner = Callable[
    [Sequence[str], int | None],
    subprocess.CompletedProcess[str],
]
CommandFinder = Callable[[str], str | None]


@dataclass(frozen=True, slots=True)
class InstallInfo:
    version: str
    source_url: str | None
    requested_revision: str | None
    commit_id: str | None
    executable: str | None


@dataclass(frozen=True, slots=True)
class RemoteRef:
    name: str
    version: tuple[int, int, int] | None
    commitish: str


@dataclass(frozen=True, slots=True)
class UpdateStatus:
    current: InstallInfo
    channel: UpdateChannel
    latest: RemoteRef | None
    state: UpdateState
    detail: str


class UpdateCheckError(RuntimeError):
    pass


def _run_command(
    command: Sequence[str], timeout: int | None = DEFAULT_TIMEOUT_SECONDS
) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        list(command),
        check=False,
        capture_output=True,
        text=True,
        timeout=timeout,
    )


def _parse_version(value: str) -> tuple[int, int, int] | None:
    match = VERSION_RE.fullmatch(value)
    if match is None:
        return None
    return (int(match.group(1)), int(match.group(2)), int(match.group(3)))


def _first_stderr_line(result: subprocess.CompletedProcess[str]) -> str:
    line = result.stderr.strip().splitlines()
    if line:
        return line[0]
    return "command failed"


def _safe_vcs_value(payload, key: str) -> str | None:
    if not isinstance(payload, dict):
        return None
    value = payload.get(key)
    return value if isinstance(value, str) else None


def read_install_info() -> InstallInfo:
    try:
        version = metadata.version("my-skills")
        distribution = metadata.distribution("my-skills")
    except metadata.PackageNotFoundError:
        version = __version__
        distribution = None

    source_url = None
    requested_revision = None
    commit_id = None
    if distribution is not None:
        raw_direct_url = distribution.read_text("direct_url.json")
        if raw_direct_url:
            try:
                direct_url = json.loads(raw_direct_url)
            except json.JSONDecodeError:
                direct_url = None
            source_url = _safe_vcs_value(direct_url, "url")
            vcs_info = direct_url.get("vcs_info") if isinstance(direct_url, dict) else None
            requested_revision = _safe_vcs_value(vcs_info, "requested_revision")
            commit_id = _safe_vcs_value(vcs_info, "commit_id")

    return InstallInfo(
        version=version,
        source_url=source_url,
        requested_revision=requested_revision,
        commit_id=commit_id,
        executable=shutil.which("my-skills"),
    )


def latest_stable_ref(
    *,
    run: CommandRunner = _run_command,
    which: CommandFinder = shutil.which,
) -> RemoteRef:
    if which("git") is None:
        raise UpdateCheckError("git not found")
    try:
        result = run(
            ["git", "ls-remote", "--tags", "--refs", REPO_URL, "v*"],
            DEFAULT_TIMEOUT_SECONDS,
        )
    except FileNotFoundError as exc:
        raise UpdateCheckError("git not found") from exc
    except subprocess.TimeoutExpired as exc:
        raise UpdateCheckError("network unavailable") from exc

    if result.returncode != 0:
        raise UpdateCheckError(_first_stderr_line(result))

    candidates: list[RemoteRef] = []
    for line in result.stdout.splitlines():
        fields = line.split()
        if len(fields) < 2:
            continue
        commitish, ref_name = fields[0], fields[1]
        match = SEMVER_TAG_RE.fullmatch(ref_name)
        if match is None:
            continue
        version = (int(match.group(1)), int(match.group(2)), int(match.group(3)))
        candidates.append(
            RemoteRef(
                name=f"v{version[0]}.{version[1]}.{version[2]}",
                version=version,
                commitish=commitish,
            )
        )

    if not candidates:
        raise UpdateCheckError("no stable release tags found")
    return max(candidates, key=lambda candidate: candidate.version or (0, 0, 0))


def latest_main_ref(
    *,
    run: CommandRunner = _run_command,
    which: CommandFinder = shutil.which,
) -> RemoteRef:
    if which("git") is None:
        raise UpdateCheckError("git not found")
    try:
        result = run(["git", "ls-remote", REPO_URL, "refs/heads/main"], DEFAULT_TIMEOUT_SECONDS)
    except FileNotFoundError as exc:
        raise UpdateCheckError("git not found") from exc
    except subprocess.TimeoutExpired as exc:
        raise UpdateCheckError("network unavailable") from exc

    if result.returncode != 0:
        raise UpdateCheckError(_first_stderr_line(result))
    fields = result.stdout.strip().split()
    if len(fields) < 2:
        raise UpdateCheckError("main branch not found")
    return RemoteRef(name="main", version=None, commitish=fields[0])


def _stable_status(current: InstallInfo, latest: RemoteRef) -> UpdateStatus:
    current_version = _parse_version(current.version)
    if latest.version is None:
        return UpdateStatus(current, "stable", latest, "not-checked", "latest version unknown")
    if current_version is None:
        return UpdateStatus(current, "stable", latest, "unknown-current", "current version unknown")
    if current_version < latest.version:
        return UpdateStatus(current, "stable", latest, "available", latest.name)
    if current_version > latest.version:
        return UpdateStatus(current, "stable", latest, "ahead", latest.name)
    return UpdateStatus(current, "stable", latest, "current", latest.name)


def _main_status(current: InstallInfo, latest: RemoteRef) -> UpdateStatus:
    if current.commit_id is None:
        return UpdateStatus(current, "main", latest, "unknown-current", "current commit unknown")
    if current.commit_id == latest.commitish:
        return UpdateStatus(current, "main", latest, "current", latest.commitish)
    return UpdateStatus(current, "main", latest, "available", latest.commitish)


def check_update(
    channel: UpdateChannel = "stable",
    *,
    install_info_reader: Callable[[], InstallInfo] | None = None,
    stable_ref_reader: Callable[[], RemoteRef] | None = None,
    main_ref_reader: Callable[[], RemoteRef] | None = None,
) -> UpdateStatus:
    if install_info_reader is None:
        install_info_reader = read_install_info
    if stable_ref_reader is None:
        stable_ref_reader = latest_stable_ref
    if main_ref_reader is None:
        main_ref_reader = latest_main_ref

    current = install_info_reader()
    try:
        match channel:
            case "stable":
                return _stable_status(current, stable_ref_reader())
            case "main":
                return _main_status(current, main_ref_reader())
    except UpdateCheckError as exc:
        return UpdateStatus(current, channel, None, "not-checked", str(exc))


def format_update_status(status: UpdateStatus) -> str:
    match status.state:
        case "available":
            if status.latest is None:
                return "Update:  not checked (latest ref unavailable)"
            return f"Update:  available {status.latest.name} (run 'my-skills update')"
        case "current":
            if status.latest is None:
                return "Update:  up to date"
            if status.current.requested_revision == "main" and status.channel == "stable":
                return f"Update:  installed from main; stable {status.latest.name} is current"
            return f"Update:  up to date ({status.channel} {status.latest.name})"
        case "ahead":
            if status.latest is None:
                return "Update:  ahead of latest release"
            return f"Update:  ahead of latest stable {status.latest.name}"
        case "not-checked":
            return f"Update:  not checked ({status.detail})"
        case "unknown-current":
            if status.latest is None:
                return f"Update:  not checked ({status.detail})"
            return f"Update:  not checked ({status.detail}; latest {status.latest.name})"


def format_doctor_update_status() -> str:
    return format_update_status(check_update())
