from __future__ import annotations

import argparse
from typing import Protocol

from .update_apply import cmd_update


class SubparserRegistry(Protocol):
    def add_parser(
        self, name: str, *, help: str | None = None
    ) -> argparse.ArgumentParser: ...


def add_update_parser(sub: SubparserRegistry) -> None:
    p_update = sub.add_parser("update", help="Update the installed my-skills CLI")
    _ = p_update.add_argument(
        "--channel",
        choices=("stable", "main"),
        default="stable",
        help="Update target (default: stable release tag)",
    )
    _ = p_update.add_argument(
        "--check",
        action="store_true",
        help="Only report whether an update is available",
    )
    _ = p_update.add_argument(
        "--dry-run",
        action="store_true",
        help="Show the update command without changing the installed CLI",
    )
    p_update.set_defaults(func=cmd_update)
