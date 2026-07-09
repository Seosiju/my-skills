from __future__ import annotations

import argparse

from . import __version__
from .audit_commands import cmd_audit
from .bootstrap_commands import cmd_bootstrap
from .cli_runtime import cmd_set_root, find_repo_root
from .inspection_commands import cmd_doctor, cmd_skills, cmd_status, cmd_validate
from .init_registry_commands import cmd_init_registry
from .install_commands import cmd_install, cmd_sync, cmd_uninstall
from .registry_commands import (
    cmd_data_path,
    cmd_disable,
    cmd_enable,
    cmd_import,
    cmd_share,
)
from .update_cli import add_update_parser

__all__ = ["build_parser", "find_repo_root", "main"]


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="my-skills",
        description="Personal cross-agent Agent Skill registry.",
    )
    _ = parser.add_argument("--version", action="version", version=f"my-skills {__version__}")
    sub = parser.add_subparsers(dest="command")

    p_validate = sub.add_parser(
        "validate", help="Validate canonical skills against the Agent Skills standard"
    )
    _ = p_validate.add_argument("skill", nargs="?", help="Skill name (default: all)")
    p_validate.set_defaults(func=cmd_validate)

    p_audit = sub.add_parser(
        "audit", help="Audit canonical skills for agent-skill security risks"
    )
    _ = p_audit.add_argument("skill", nargs="?", help="Skill name (default: enabled skills)")
    _ = p_audit.add_argument("--all", action="store_true", help="Include every registered skill")
    _ = p_audit.add_argument("--json", action="store_true", help="Print machine-readable JSON")
    p_audit.set_defaults(func=cmd_audit)

    p_doctor = sub.add_parser(
        "doctor", help="Report environment, hosts, and manifest health"
    )
    _ = p_doctor.add_argument(
        "--no-update-check",
        action="store_true",
        help="Skip checking the latest released CLI version",
    )
    p_doctor.set_defaults(func=cmd_doctor)

    add_update_parser(sub)

    p_bootstrap = sub.add_parser(
        "bootstrap",
        help="Set up the CLI command and install enabled skills for this machine (contributor/dev only)",
        description="Set up the CLI command and install enabled skills for this machine (contributor/dev only)",
    )
    _ = p_bootstrap.add_argument(
        "--dry-run",
        action="store_true",
        help="Show bootstrap actions and the skill install plan; change nothing",
    )
    _ = p_bootstrap.add_argument(
        "--skip-tool-install",
        action="store_true",
        help="Skip uv tool install for the my-skills command",
    )
    _ = p_bootstrap.add_argument(
        "--skip-skill-install",
        action="store_true",
        help="Skip installing enabled skills into agent hosts",
    )
    _ = p_bootstrap.add_argument(
        "--host",
        default="all",
        help="Target host name, or 'all' (default: all enabled targets)",
    )
    _ = p_bootstrap.add_argument(
        "--all",
        action="store_true",
        help="Include every registered skill instead of only enabled skills",
    )
    _ = p_bootstrap.add_argument(
        "--mode",
        choices=("copy", "link"),
        default="copy",
        help="Skill install mode",
    )
    p_bootstrap.set_defaults(func=cmd_bootstrap)

    p_init = sub.add_parser(
        "init-registry",
        help="Create a private canonical skill registry scaffold",
    )
    _ = p_init.add_argument("path", nargs="?", help="Directory to create or initialize")
    default_seed = p_init.add_mutually_exclusive_group()
    _ = default_seed.add_argument(
        "--with-defaults",
        dest="with_defaults",
        action="store_true",
        default=True,
        help="Seed public-safe default skills into the new registry (default)",
    )
    _ = default_seed.add_argument(
        "--no-defaults",
        dest="with_defaults",
        action="store_false",
        help="Create an empty registry without seeded default skills",
    )
    _ = p_init.add_argument(
        "--no-git",
        action="store_true",
        help="Skip git init for the new registry",
    )
    p_init.set_defaults(func=cmd_init_registry)

    p_set_root = sub.add_parser(
        "set-root",
        help="Set the active registry root used when running outside a registry",
    )
    _ = p_set_root.add_argument("path", nargs="?", help="Registry root path (default: cwd)")
    p_set_root.set_defaults(func=cmd_set_root)

    p_install = sub.add_parser(
        "install", help="Install skills into host directories (copy mode)"
    )
    _ = p_install.add_argument("skill", nargs="?", help="Skill name (default: enabled skills)")
    _ = p_install.add_argument("--host", help="Target host name, or 'all' (default: enabled targets)")
    _ = p_install.add_argument("--all", action="store_true", help="Include every registered skill")
    _ = p_install.add_argument("--mode", choices=("copy", "link"), default="copy", help="Install mode")
    _ = p_install.add_argument("--dry-run", action="store_true", help="Show the plan; change nothing")
    _ = p_install.add_argument("--json", action="store_true", help="Print dry-run plan as JSON")
    _ = p_install.add_argument(
        "--skip-audit",
        action="store_true",
        help="Explicitly bypass the audit gate before writing",
    )
    _ = p_install.add_argument(
        "--yes",
        action="store_true",
        help="Confirm writing to multiple host directories",
    )
    p_install.set_defaults(func=cmd_install)

    p_share = sub.add_parser("share", help="Plan sharing host-local skills into canonical")
    _ = p_share.add_argument("skill", nargs="?", help="Host skill name")
    _ = p_share.add_argument("--from", dest="from_host", required=True, help="Source host name")
    _ = p_share.add_argument("--plan", action="store_true", help="Show candidate plan; change nothing")
    _ = p_share.add_argument("--json", action="store_true", help="Print machine-readable JSON")
    share_state = p_share.add_mutually_exclusive_group()
    _ = share_state.add_argument("--enable", action="store_true", help="Register shared skill as enabled")
    _ = share_state.add_argument("--disable", action="store_true", help="Register shared skill as disabled")
    _ = p_share.add_argument(
        "--force",
        action="store_true",
        help="Overwrite an existing different canonical skill",
    )
    _ = p_share.add_argument(
        "--skip-audit",
        action="store_true",
        help="Explicitly bypass the audit gate before writing",
    )
    p_share.set_defaults(func=cmd_share)

    p_enable = sub.add_parser("enable", help="Enable a registered skill in the manifest")
    _ = p_enable.add_argument("skill", help="Skill name")
    p_enable.set_defaults(func=cmd_enable)

    p_disable = sub.add_parser("disable", help="Disable a registered skill in the manifest")
    _ = p_disable.add_argument("skill", help="Skill name")
    p_disable.set_defaults(func=cmd_disable)

    p_status = sub.add_parser("status", help="Show install status per skill and host")
    p_status.set_defaults(func=cmd_status)

    p_skills = sub.add_parser("skills", help="List registered canonical skills")
    _ = p_skills.add_argument("--json", action="store_true", help="Print machine-readable JSON")
    _ = p_skills.add_argument("--host", help="Show skills compatible with a host")
    state_filter = p_skills.add_mutually_exclusive_group()
    _ = state_filter.add_argument("--enabled", action="store_true", help="Only enabled skills")
    _ = state_filter.add_argument("--disabled", action="store_true", help="Only disabled skills")
    _ = p_skills.add_argument(
        "--with-status",
        action="store_true",
        help="(deprecated) install status is always shown",
    )
    p_skills.set_defaults(func=cmd_skills)

    p_sync = sub.add_parser("sync", help="Update managed installs from canonical (copy mode)")
    _ = p_sync.add_argument("skill", nargs="?", help="Skill name (default: enabled skills)")
    _ = p_sync.add_argument("--host", help="Target host name, or 'all' (default: enabled targets)")
    _ = p_sync.add_argument("--all", action="store_true", help="Include every registered skill")
    _ = p_sync.add_argument("--check", action="store_true", help="Report drift only; change nothing")
    _ = p_sync.add_argument(
        "--yes",
        action="store_true",
        help="Confirm writing to multiple host directories",
    )
    _ = p_sync.add_argument(
        "--skip-audit",
        action="store_true",
        help="Explicitly bypass the audit gate before writing",
    )
    p_sync.set_defaults(func=cmd_sync)

    p_uninstall = sub.add_parser("uninstall", help="Remove managed installs")
    _ = p_uninstall.add_argument("skill", nargs="?", help="Skill name")
    _ = p_uninstall.add_argument("--host", help="Target host name, or 'all' (default: enabled targets)")
    _ = p_uninstall.add_argument(
        "--yes",
        action="store_true",
        help="Confirm removing from multiple host directories",
    )
    p_uninstall.set_defaults(func=cmd_uninstall)

    p_import = sub.add_parser(
        "import", help="Import an external skill directory into canonical skills/"
    )
    _ = p_import.add_argument("source", help="Path to a skill directory (contains SKILL.md)")
    _ = p_import.add_argument(
        "--force", action="store_true", help="Overwrite an existing different canonical skill"
    )
    _ = p_import.add_argument(
        "--enable",
        action="store_true",
        help="Register the skill as enabled (default: registered but disabled)",
    )
    _ = p_import.add_argument(
        "--skip-audit",
        action="store_true",
        help="Explicitly bypass the audit gate before writing",
    )
    p_import.set_defaults(func=cmd_import)

    p_data = sub.add_parser(
        "data-path",
        help="Print a skill's shared machine-local data directory",
    )
    _ = p_data.add_argument("skill", help="Skill name")
    _ = p_data.add_argument(
        "--create", action="store_true", help="Create the directory if missing"
    )
    p_data.set_defaults(func=cmd_data_path)

    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    if not getattr(args, "command", None):
        parser.print_help()
        return 0
    handler = getattr(args, "func", None)
    if not callable(handler):
        parser.print_help()
        return 2
    result = handler(args)
    if isinstance(result, int):
        return result
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
