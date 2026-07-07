from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import sys
from pathlib import Path

from .cli_runtime import find_repo_root
from .config import ManifestError
from .install_commands import cmd_install


def _tool_install_command(uv: str, root: Path) -> list[str]:
    return [
        uv,
        "tool",
        "install",
        "--editable",
        str(root),
        "--force",
        "--link-mode=copy",
    ]


def _format_command(command: list[str]) -> str:
    return " ".join(command)


def _run_tool_install(root: Path) -> int:
    uv = shutil.which("uv")
    if uv is None:
        print(
            "error: uv command not found; install uv first, then rerun bootstrap.",
            file=sys.stderr,
        )
        return 2
    return subprocess.run(_tool_install_command(uv, root)).returncode


def _install_args(args: argparse.Namespace) -> argparse.Namespace:
    return argparse.Namespace(
        skill=None,
        host=args.host,
        all=args.all,
        mode=args.mode,
        dry_run=False,
        json=False,
        yes=True,
        skip_audit=False,
    )


def _dry_run_install_command(args: argparse.Namespace) -> list[str]:
    command = ["my-skills", "install", "--host", args.host, "--mode", args.mode]
    if args.all:
        command.append("--all")
    command.extend(["--yes", "--dry-run"])
    return command


def _install_skills(args: argparse.Namespace, root: Path) -> int:
    previous = os.environ.get("MY_SKILLS_ROOT")
    os.environ["MY_SKILLS_ROOT"] = str(root)
    try:
        return cmd_install(_install_args(args))
    finally:
        if previous is None:
            os.environ.pop("MY_SKILLS_ROOT", None)
        else:
            os.environ["MY_SKILLS_ROOT"] = previous


def cmd_bootstrap(args: argparse.Namespace) -> int:
    try:
        root = find_repo_root(write_cache=False).resolve()
    except ManifestError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2

    if args.dry_run:
        print("Dry run - bootstrap actions (nothing installed):")
        if not args.skip_tool_install:
            command = _tool_install_command("uv", root)
            print(f"  would run: {_format_command(command)}")
        if not args.skip_skill_install:
            preview = _dry_run_install_command(args)
            print(f"  would run: {_format_command(preview)}")
        return 0

    if not args.skip_tool_install:
        print("Installing my-skills CLI with uv tool install...")
        rc = _run_tool_install(root)
        if rc != 0:
            return rc

    if not args.skip_skill_install:
        print("Installing enabled skills into agent hosts...")
        rc = _install_skills(args, root)
        if rc != 0:
            return rc

    print("Bootstrap complete. Run `my-skills doctor` to inspect registry configuration.")
    return 0
