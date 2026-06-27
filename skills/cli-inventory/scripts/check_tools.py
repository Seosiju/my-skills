#!/usr/bin/env python3
"""Check that the tools named in the required-tool policy resolve on PATH.

Host-neutral and stdlib-only. Reports which required/optional tools are present
and which are missing, then exits non-zero if any *required* tool is missing.

This script never installs anything: it only reports. Machine-specific results
(absolute paths, versions, auth state) are printed for the caller to act on and
should be persisted, if at all, under the git-ignored ``local/cli-inventory/``
directory described in ``references/required-tools.md`` — never committed.

Keep this list in sync with ``references/required-tools.md``.
"""

from __future__ import annotations

import shutil
import sys

REQUIRED = ("git", "python3", "uv")
OPTIONAL = ("gh", "rg")


def resolve(tools: tuple[str, ...]) -> dict[str, str | None]:
    """Map each tool name to its resolved PATH location, or None if missing."""
    return {tool: shutil.which(tool) for tool in tools}


def report(label: str, resolved: dict[str, str | None]) -> list[str]:
    """Print a present/missing line per tool; return the names that are missing."""
    missing: list[str] = []
    for tool, location in resolved.items():
        if location:
            print(f"  [present] {tool} -> {location}")
        else:
            print(f"  [missing] {tool}")
            missing.append(tool)
    return missing


def main() -> int:
    print("Required tools:")
    missing_required = report("required", resolve(REQUIRED))
    print("Optional tools:")
    report("optional", resolve(OPTIONAL))

    if missing_required:
        print(
            f"\nMissing required tools: {', '.join(missing_required)}. "
            "Install them and re-run; this script does not install anything.",
            file=sys.stderr,
        )
        return 1
    print("\nAll required tools are available.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
