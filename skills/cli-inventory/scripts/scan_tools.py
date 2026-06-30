#!/usr/bin/env python3
"""Discover the command-line tools installed on this machine and record them.

Host-neutral and stdlib-only. The scan walks every directory on ``PATH`` for
executables and queries any package managers that are present (Homebrew, npm
global, pipx, cargo, gem, pip) for what they installed. It then writes the
result to the shared machine-local data root so later lookups are fast.

Two files are written to ``<data-root>/cli-inventory/``:

  - ``inventory.json`` — the complete record (every PATH executable + manager
    package lists), for programmatic use.
  - ``inventory.md`` — a readable summary: totals, an "expected tools" check,
    and the curated list of manager-installed packages.

The data root is machine-local and is **never committed** (it is the same root
``personal-profile`` uses). This script only reads the machine and writes the
inventory there; it installs nothing.

Resolve the data root host-neutrally, matching the ``my-skills`` rule:
``$XDG_DATA_HOME/my-skills/cli-inventory`` if ``XDG_DATA_HOME`` is set, else
``~/.local/share/my-skills/cli-inventory``.

See ``references/inventory-schema.md`` for the output format and the
machine-local boundary.
"""

from __future__ import annotations

import json
import os
import platform
import shutil
import socket
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

# A small set of tools many workflows expect. The scan reports each as present
# or missing so a caller can act on a gap. Edit this set as your workflows
# change; it only drives the "expected tools" highlight, not the full scan.
EXPECTED = ("git", "python3", "uv", "node", "npm", "gh", "rg", "jq")

# The agent CLI-service catalog: the curated whitelist of CLIs you actually use
# to drive a service (GitHub, Jira, a cloud, an MCP/agent tool, ...). Each entry
# is (name, service) — both low-churn facts you maintain by hand. The scan fills
# the volatile parts automatically per machine (installed?, path, version,
# source), so a later lookup reads one compact table instead of re-probing each
# tool. Add or remove a row as your toolkit changes; a name that is not installed
# is reported as missing rather than silently dropped.
CATALOG: tuple[tuple[str, str], ...] = (
    ("gh", "GitHub"),
    ("twg", "Atlassian (Teamwork Graph)"),
    ("claude", "Anthropic Claude Code"),
    ("codex", "OpenAI Codex"),
    ("gemini", "Google Gemini"),
    ("gcloud", "Google Cloud"),
    ("kubectl", "Kubernetes"),
    ("gws", "Google Workspace"),
    ("supabase", "Supabase"),
    ("codegraph", "CodeGraph (local code intel)"),
    ("my-skills", "my-skills registry (local)"),
)

# Tried in order to read a tool's version; the first that prints a non-empty
# line wins. Tools disagree on the flag, so we probe rather than assume.
VERSION_FLAGS = ("--version", "version", "-v")

# Package managers to query, mapped to the argv that lists installed packages.
# A manager that is not on PATH is skipped silently.
MANAGERS: dict[str, list[str]] = {
    "brew": ["brew", "list", "--versions"],
    "npm": ["npm", "-g", "ls", "--depth=0"],
    "pipx": ["pipx", "list", "--short"],
    "cargo": ["cargo", "install", "--list"],
    "gem": ["gem", "list", "--local"],
    "pip": ["pip", "list", "--format=freeze"],
}


def data_root() -> Path:
    """Resolve the shared machine-local data root for this skill."""
    xdg = os.environ.get("XDG_DATA_HOME")
    base = Path(xdg) if xdg else Path.home() / ".local" / "share"
    return base / "my-skills" / "cli-inventory"


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


# First word of a line that signals an error/usage message rather than a
# version, so an unsupported flag does not get recorded as the version.
_ERROR_PREFIXES = ("error", "usage", "unknown", "invalid", "no ", "command ")


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
    so an unsupported flag is not mistaken for a version. Bounded by a short
    timeout so a tool that blocks does not stall the scan.
    """
    for flag in VERSION_FLAGS:
        try:
            proc = subprocess.run(
                [name, flag],
                capture_output=True,
                text=True,
                timeout=6,
                check=False,
            )
        except (OSError, subprocess.TimeoutExpired):
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
    """Coarsely label where a resolved executable comes from, for evidence.

    The path itself is the evidence; this just names the obvious origins so a
    reader can tell a Homebrew bin from an npm/node bin at a glance.
    """
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


def resolve_catalog(path_tools: dict[str, str]) -> list[dict]:
    """Resolve every catalog entry against this machine.

    For each (name, service) the scan records whether it is installed, where it
    resolves, its version, and a coarse source label. Volatile facts only — the
    name and service come from ``CATALOG`` and are left as authored.
    """
    rows: list[dict] = []
    for name, service in CATALOG:
        path = path_tools.get(name) or shutil.which(name)
        rows.append(
            {
                "name": name,
                "service": service,
                "status": "available" if path else "missing",
                "path": path,
                "version": detect_version(name) if path else None,
                "source": classify_source(path) if path else None,
            }
        )
    return rows


def query_manager(name: str, argv: list[str]) -> list[str] | None:
    """Return a manager's package lines, or None if the manager is unavailable."""
    if shutil.which(argv[0]) is None:
        return None
    try:
        proc = subprocess.run(
            argv,
            capture_output=True,
            text=True,
            timeout=60,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired):
        return []
    lines = [line.strip() for line in proc.stdout.splitlines() if line.strip()]
    return lines


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
    expected = {tool: path_tools.get(tool) or shutil.which(tool) for tool in EXPECTED}
    return {
        "generated": datetime.now(timezone.utc).isoformat(timespec="seconds"),
        "host": socket.gethostname(),
        "platform": platform.platform(),
        "catalog": resolve_catalog(path_tools),
        "expected": expected,
        "path_tools": path_tools,
        "managers": scan_managers(),
    }


def render_markdown(inv: dict) -> str:
    """Render the readable summary written to inventory.md."""
    lines: list[str] = ["# CLI inventory", ""]
    lines.append(f"- Generated: {inv['generated']}")
    lines.append(f"- Host: {inv['host']}")
    lines.append(f"- Platform: {inv['platform']}")
    lines.append(f"- Executables on PATH: {len(inv['path_tools'])}")
    lines.append("")

    catalog = inv.get("catalog", [])
    available = sum(1 for r in catalog if r["status"] == "available")
    lines.append(f"## Agent CLI services ({available}/{len(catalog)} available)")
    lines.append("")
    lines.append("| name | service | status | version | source | path |")
    lines.append("| --- | --- | --- | --- | --- | --- |")
    for r in catalog:
        lines.append(
            f"| {r['name']} | {r['service']} | {r['status']} "
            f"| {r['version'] or '—'} | {r['source'] or '—'} | {r['path'] or '—'} |"
        )
    lines.append("")
    lines.append(
        "_Catalog rows are the CLIs you curate in the `CATALOG` list; the scan "
        "fills status/version/source/path per machine._"
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
    lines.append(
        "_Full package lists and the complete PATH executable map "
        "(`path_tools`) are in `inventory.json`; this summary keeps only counts "
        "so a lookup stays cheap to read._"
    )
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

    catalog = inv.get("catalog", [])
    available = sum(1 for r in catalog if r["status"] == "available")
    missing = [t for t, loc in inv["expected"].items() if not loc]
    print(f"Scanned {len(inv['path_tools'])} executables on PATH.")
    print(f"Agent CLI services: {available}/{len(catalog)} available.")
    print(f"Package managers queried: {', '.join(inv['managers']) or 'none'}.")
    if missing:
        print(f"Expected tools missing: {', '.join(missing)}.")
    else:
        print("All expected tools are present.")
    print(f"Inventory written to {root}/inventory.md and inventory.json")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
