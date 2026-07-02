#!/usr/bin/env python3
"""Discover the CLI commands you deliberately installed on this machine.

Host-neutral and stdlib-only. Rather than dump every one of the thousands of
executables on ``PATH`` (mostly libraries, language shims, and system scripts),
this scans the directories where *deliberate* installs land — your user bin
directories (``~/.local/bin``, ``~/bin``, ``~/.cargo/bin``, the active node bin)
and top-level (leaf) Homebrew formulae — and lists what it finds with each
command's version, source, and path. That set is a few dozen entries, not
thousands, so the readable summary stays cheap to read and nothing you installed
is hidden behind a hand-curated list.

A new install lands in one of these directories, so it shows up automatically on
the next scan — no catalog to maintain. Commands matching ``EXCLUDE`` (internal
hook scripts) are left out, and ``SERVICE_LABELS`` optionally sorts the ones you
care about into a "Service CLIs" highlight; both are easy to edit.

Two files are written to ``<data-root>/cli-inventory/``:

  - ``inventory.json`` — the complete record: the resolved command list plus the
    full raw scan (every PATH executable + manager package lists), for
    programmatic use.
  - ``inventory.md`` — a readable summary: the command tables, an "expected
    tools" check, and per-manager package counts.

The data root is machine-local and is **never committed** (the same root
``personal-profile`` uses). This script only reads the machine and writes the
inventory there; it installs nothing.

Resolve the data root host-neutrally, matching the ``my-skills`` rule:
``$XDG_DATA_HOME/my-skills/cli-inventory`` if ``XDG_DATA_HOME`` is set, else
``~/.local/share/my-skills/cli-inventory``.

See ``references/inventory-schema.md`` for the output format and the
machine-local boundary.
"""

from __future__ import annotations

import fnmatch
import json
import os
import platform
import shutil
import socket
import subprocess
from datetime import datetime, timezone
from pathlib import Path

# A small set of tools many workflows expect. The scan reports each as present
# or missing so a caller can act on a gap. Edit this set as your workflows
# change; it only drives the "expected tools" highlight, not the command scan.
EXPECTED = ("git", "python3", "uv", "node", "npm", "gh", "rg", "jq")

# Optional annotation: command name -> the service it drives. Purely cosmetic —
# a command is listed whether or not it appears here; a label just sorts it into
# the "Service CLIs" highlight instead of "Other installed commands". Add a line
# when you want a command called out. Nothing is hidden by leaving one out.
SERVICE_LABELS: dict[str, str] = {
    "gh": "GitHub",
    "twg": "Atlassian (Teamwork Graph)",
    "claude": "Anthropic Claude Code",
    "codex": "OpenAI Codex",
    "gemini": "Google Gemini",
    "gcloud": "Google Cloud",
    "kubectl": "Kubernetes",
    "gws": "Google Workspace",
    "supabase": "Supabase",
    "codegraph": "CodeGraph (local code intel)",
    "my-skills": "my-skills registry (local)",
    "hermes": "Hermes (local agent)",
    "graphify": "graphify (local)",
}

# Commands matching any of these glob patterns are treated as internal noise and
# left out of the listing (e.g. oh-my-claudecode's omo-* hook scripts). Edit to
# taste; a pattern is matched against the command name only.
EXCLUDE = ("omo-*",)

# Tried in order to read a tool's version; the first that prints a non-error
# line wins. Tools disagree on the flag, so we probe rather than assume.
VERSION_FLAGS = ("--version", "version", "-v")

# First word of a line that signals an error/usage message rather than a
# version, so an unsupported flag does not get recorded as the version.
_ERROR_PREFIXES = ("error", "usage", "unknown", "invalid", "no ", "command ")


def data_root() -> Path:
    """Resolve the shared machine-local data root for this skill."""
    xdg = os.environ.get("XDG_DATA_HOME")
    base = Path(xdg) if xdg else Path.home() / ".local" / "share"
    return base / "my-skills" / "cli-inventory"


def _run(argv: list[str], timeout: int = 4) -> subprocess.CompletedProcess | None:
    """Run a command capturing output; None on failure/timeout. stdin is closed
    so a tool that waits on input gets EOF instead of hanging the scan."""
    try:
        return subprocess.run(
            argv,
            capture_output=True,
            text=True,
            errors="replace",
            timeout=timeout,
            stdin=subprocess.DEVNULL,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired):
        return None


def _is_version_line(line: str) -> bool:
    """True if a line looks like a real version, not an error/usage message."""
    low = line.lower()
    if not line or low.startswith(_ERROR_PREFIXES):
        return False
    return "unknown flag" not in low and "unrecognized" not in low


def detect_version(name: str) -> str | None:
    """Return a tool's reported version line, or None if it gives nothing.

    Each flag in ``VERSION_FLAGS`` is tried in order; stdout is preferred over
    stderr, and any line that looks like an error or usage message is rejected
    so an unsupported flag is not mistaken for a version.
    """
    for flag in VERSION_FLAGS:
        proc = _run([name, flag])
        if proc is None:
            continue
        for stream in (proc.stdout, proc.stderr):
            for raw in stream.splitlines():
                line = raw.strip()
                if line:
                    if _is_version_line(line):
                        return line
                    break  # first non-empty line decides this stream
    return None


def classify_source(path: str) -> str:
    """Coarsely label where a resolved executable comes from, for evidence."""
    p = path.lower()
    if "/homebrew/" in p or p.startswith("/usr/local/cellar"):
        return "brew"
    if "/.nvm/" in p or "/node_modules/" in p:
        return "npm"
    if "/anaconda" in p or "/miniconda" in p or "/site-packages/" in p:
        return "conda/pip"
    if "/.local/bin/" in p:
        return "local"
    if "/.cargo/" in p:
        return "cargo"
    return "PATH"


def scan_path() -> dict[str, str]:
    """Map each executable name on PATH to its resolved location (first wins)."""
    found: dict[str, str] = {}
    seen_dirs: set[str] = set()
    for raw in os.environ.get("PATH", "").split(os.pathsep):
        if not raw or raw in seen_dirs:
            continue
        seen_dirs.add(raw)
        directory = Path(raw)
        if not directory.is_dir():
            continue
        try:
            entries = list(directory.iterdir())
        except OSError:
            continue
        for entry in entries:
            name = entry.name
            if name in found:  # earlier PATH dir takes precedence
                continue
            if os.access(entry, os.X_OK) and entry.is_file():
                found[name] = str(entry)
    return dict(sorted(found.items()))


def install_dirs() -> list[Path]:
    """Directories where deliberate user installs land."""
    home = Path.home()
    dirs = [home / ".local" / "bin", home / "bin", home / ".cargo" / "bin"]
    node = shutil.which("node")
    if node:  # the active node/nvm bin holds globally-installed JS CLIs
        dirs.append(Path(node).resolve().parent)
    return dirs


def scan_install_dirs() -> dict[str, str]:
    """List executables found in the deliberate-install directories."""
    found: dict[str, str] = {}
    for directory in install_dirs():
        if not directory.is_dir():
            continue
        try:
            entries = list(directory.iterdir())
        except OSError:
            continue
        for entry in entries:
            if entry.name in found:
                continue
            if os.access(entry, os.X_OK) and entry.is_file():
                found[entry.name] = str(entry)
    return found


def _excluded(name: str) -> bool:
    """True if a command name matches any EXCLUDE glob pattern."""
    return any(fnmatch.fnmatch(name, pat) for pat in EXCLUDE)


def resolve_service_clis() -> list[dict]:
    """Resolve the labelled service CLIs that are actually installed.

    Driven by ``SERVICE_LABELS`` and resolved via ``which`` so the command name
    is what you invoke (``gws``), independent of the package that ships it.
    """
    rows: list[dict] = []
    for name, service in SERVICE_LABELS.items():
        path = shutil.which(name)
        if not path:
            continue  # labelled but not installed here — just an annotation
        rows.append(
            {
                "name": name,
                "service": service,
                "version": detect_version(name),
                "source": classify_source(path),
                "path": path,
            }
        )
    return sorted(rows, key=lambda r: r["name"])


def resolve_user_commands(labelled: set[str]) -> list[dict]:
    """Individually-installed binaries in the user bin directories.

    These are atomic installs (one file = one tool: ``twg``, ``mise``, ``node``),
    unlike package-manager formulae which can ship dozens of binaries. Labelled
    service CLIs and EXCLUDE matches are left out (services are listed
    separately; EXCLUDE drops internal hook scripts).
    """
    found = scan_install_dirs()
    rows: list[dict] = []
    for name in sorted(found):
        if _excluded(name) or name in labelled:
            continue
        path = shutil.which(name) or found[name]
        rows.append(
            {
                "name": name,
                "version": detect_version(name),
                "source": classify_source(path),
                "path": path,
            }
        )
    return rows


def brew_leaf_tools(brew_lines: list[str]) -> list[dict]:
    """Top-level (leaf) Homebrew formulae as one row each — name + version.

    The *formula* is the deliberate install unit, so coreutils is one row, not
    its ~100 binaries. Versions come from ``brew list --versions`` output that
    the manager scan already collected, so nothing is probed here.
    """
    if shutil.which("brew") is None:
        return []
    leaves_proc = _run(["brew", "leaves"], timeout=15)
    if leaves_proc is None:
        return []
    leaves = {line.strip() for line in leaves_proc.stdout.splitlines() if line.strip()}
    if not leaves:
        return []
    versions: dict[str, str] = {}
    for line in brew_lines:  # "coreutils 9.11" (from `brew list --versions`)
        parts = line.split()
        if parts:
            versions[parts[0]] = parts[1] if len(parts) > 1 else None
    return [
        {"name": name, "version": versions.get(name)} for name in sorted(leaves)
    ]


def query_manager(name: str, argv: list[str]) -> list[str] | None:
    """Return a manager's package lines, or None if the manager is unavailable."""
    if shutil.which(argv[0]) is None:
        return None
    try:
        proc = subprocess.run(
            argv,
            capture_output=True,
            text=True,
            errors="replace",
            timeout=60,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired):
        return []
    lines = [line.strip() for line in proc.stdout.splitlines() if line.strip()]
    return lines


# Package managers to query for the raw record, mapped to the argv that lists
# installed packages. A manager that is not on PATH is skipped silently.
MANAGERS: dict[str, list[str]] = {
    "brew": ["brew", "list", "--versions"],
    "npm": ["npm", "-g", "ls", "--depth=0"],
    "pipx": ["pipx", "list", "--short"],
    "cargo": ["cargo", "install", "--list"],
    "gem": ["gem", "list", "--local"],
    "pip": ["pip", "list", "--format=freeze"],
}


def scan_managers() -> dict[str, list[str]]:
    """Query every present package manager for its installed packages."""
    result: dict[str, list[str]] = {}
    for name, argv in MANAGERS.items():
        lines = query_manager(name, argv)
        if lines is not None:
            result[name] = lines
    return result


def build_inventory() -> dict:
    """Assemble the full inventory record."""
    path_tools = scan_path()
    managers = scan_managers()
    expected = {tool: path_tools.get(tool) or shutil.which(tool) for tool in EXPECTED}
    service_clis = resolve_service_clis()
    labelled = {r["name"] for r in service_clis}
    return {
        "generated": datetime.now(timezone.utc).isoformat(timespec="seconds"),
        "host": socket.gethostname(),
        "platform": platform.platform(),
        "service_clis": service_clis,
        "user_commands": resolve_user_commands(labelled),
        "brew_leaves": brew_leaf_tools(managers.get("brew", [])),
        "expected": expected,
        "path_tools": path_tools,
        "managers": managers,
    }


def _command_row(cols: list[str]) -> str:
    return "| " + " | ".join(cols) + " |"


def render_markdown(inv: dict) -> str:
    """Render the readable summary written to inventory.md."""
    lines: list[str] = ["# CLI inventory", ""]
    lines.append(f"- Generated: {inv['generated']}")
    lines.append(f"- Host: {inv['host']}")
    lines.append(f"- Platform: {inv['platform']}")
    lines.append(f"- Executables on PATH: {len(inv['path_tools'])}")
    lines.append("")

    services = inv.get("service_clis", [])
    user_cmds = inv.get("user_commands", [])
    brew_leaves = inv.get("brew_leaves", [])

    lines.append(f"## Service CLIs ({len(services)})")
    lines.append("")
    lines.append("| name | service | version | source | path |")
    lines.append("| --- | --- | --- | --- | --- |")
    for c in services:
        lines.append(
            _command_row(
                [c["name"], c["service"], c["version"] or "—", c["source"], c["path"]]
            )
        )
    lines.append("")
    lines.append(
        "_Labelled in `SERVICE_LABELS` and resolved live; a label that is not "
        "installed here is simply omitted. Nothing else is hidden — unlabelled "
        "tools appear below._"
    )
    lines.append("")

    lines.append(f"## Other user-installed commands ({len(user_cmds)})")
    lines.append("")
    lines.append("| name | version | source | path |")
    lines.append("| --- | --- | --- | --- |")
    for c in user_cmds:
        lines.append(
            _command_row([c["name"], c["version"] or "—", c["source"], c["path"]])
        )
    lines.append("")
    lines.append(
        "_Individual binaries in your user bin dirs (`~/.local/bin`, `~/bin`, "
        "`~/.cargo/bin`, the active node bin), minus `EXCLUDE` and the labelled "
        "services above._"
    )
    lines.append("")

    lines.append(f"## Homebrew tools — leaf formulae ({len(brew_leaves)})")
    lines.append("")
    lines.append("| name | version |")
    lines.append("| --- | --- |")
    for c in brew_leaves:
        lines.append(_command_row([c["name"], c["version"] or "—"]))
    lines.append("")
    lines.append(
        "_Top-level formulae you installed (dependencies excluded). One row per "
        "formula, not per shipped binary, so a multi-binary formula like "
        "`coreutils` counts once. Their commands live on PATH; see "
        "`inventory.json` `path_tools` for the full map._"
    )
    lines.append("")

    lines.append("## Expected tools")
    lines.append("")
    for tool, location in inv["expected"].items():
        mark = location if location else "MISSING"
        lines.append(f"- {tool}: {mark}")
    lines.append("")

    lines.append("## Installed packages (counts)")
    lines.append("")
    if not inv["managers"]:
        lines.append("_No supported package manager found on PATH._")
    else:
        for manager, packages in inv["managers"].items():
            lines.append(f"- {manager}: {len(packages)}")
    lines.append("")
    return "\n".join(lines)


def main() -> int:
    root = data_root()
    root.mkdir(parents=True, exist_ok=True)

    inv = build_inventory()
    (root / "inventory.json").write_text(
        json.dumps(inv, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
    )
    (root / "inventory.md").write_text(render_markdown(inv), encoding="utf-8")

    missing = [t for t, loc in inv["expected"].items() if not loc]
    print(f"Scanned {len(inv['path_tools'])} executables on PATH.")
    print(
        f"Service CLIs: {len(inv['service_clis'])} · "
        f"other user commands: {len(inv['user_commands'])} · "
        f"Homebrew leaves: {len(inv['brew_leaves'])}."
    )
    print(f"Package managers queried: {', '.join(inv['managers']) or 'none'}.")
    if missing:
        print(f"Expected tools missing: {', '.join(missing)}.")
    else:
        print("All expected tools are present.")
    print(f"Inventory written to {root}/inventory.md and inventory.json")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
