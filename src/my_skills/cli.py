from __future__ import annotations

import argparse

from .audit_commands import cmd_audit
from .bootstrap_commands import cmd_bootstrap
from .cli_runtime import find_repo_root
from .inspection_commands import cmd_doctor, cmd_skills, cmd_status, cmd_validate
from .install_commands import cmd_install, cmd_sync, cmd_uninstall
from .registry_commands import (
    cmd_data_path,
    cmd_disable,
    cmd_enable,
    cmd_import,
    cmd_share,
)

__all__ = ["build_parser", "find_repo_root", "main"]


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="my-skills",
        description="Personal cross-agent Agent Skill registry.",
    )
    sub = parser.add_subparsers(dest="command")

    p_validate = sub.add_parser(
        "validate", help="Validate canonical skills against the Agent Skills standard"
    )
    p_validate.add_argument("skill", nargs="?", help="Skill name (default: all)")
    p_validate.set_defaults(func=cmd_validate)

    p_audit = sub.add_parser(
        "audit", help="Audit canonical skills for agent-skill security risks"
    )
    p_audit.add_argument("skill", nargs="?", help="Skill name (default: enabled skills)")
    p_audit.add_argument("--all", action="store_true", help="Include every registered skill")
    p_audit.add_argument("--json", action="store_true", help="Print machine-readable JSON")
    p_audit.set_defaults(func=cmd_audit)

    p_doctor = sub.add_parser(
        "doctor", help="Report environment, hosts, and manifest health"
    )
    p_doctor.set_defaults(func=cmd_doctor)

    p_bootstrap = sub.add_parser(
        "bootstrap",
        help="Set up the CLI command and install enabled skills for this machine",
    )
    p_bootstrap.add_argument(
        "--dry-run",
        action="store_true",
        help="Show bootstrap actions and the skill install plan; change nothing",
    )
    p_bootstrap.add_argument(
        "--skip-tool-install",
        action="store_true",
        help="Skip uv tool install for the my-skills command",
    )
    p_bootstrap.add_argument(
        "--skip-skill-install",
        action="store_true",
        help="Skip installing enabled skills into agent hosts",
    )
    p_bootstrap.add_argument(
        "--host",
        default="all",
        help="Target host name, or 'all' (default: all enabled targets)",
    )
    p_bootstrap.add_argument(
        "--all",
        action="store_true",
        help="Include every registered skill instead of only enabled skills",
    )
    p_bootstrap.add_argument(
        "--mode",
        choices=("copy", "link"),
        default="copy",
        help="Skill install mode",
    )
    p_bootstrap.set_defaults(func=cmd_bootstrap)

    p_install = sub.add_parser(
        "install", help="Install skills into host directories (copy mode)"
    )
    p_install.add_argument("skill", nargs="?", help="Skill name (default: enabled skills)")
    p_install.add_argument("--host", help="Target host name, or 'all' (default: enabled targets)")
    p_install.add_argument("--all", action="store_true", help="Include every registered skill")
    p_install.add_argument("--mode", choices=("copy", "link"), default="copy", help="Install mode")
    p_install.add_argument("--dry-run", action="store_true", help="Show the plan; change nothing")
    p_install.add_argument("--json", action="store_true", help="Print dry-run plan as JSON")
    p_install.add_argument(
        "--skip-audit",
        action="store_true",
        help="Explicitly bypass the audit gate before writing",
    )
    p_install.add_argument(
        "--yes",
        action="store_true",
        help="Confirm writing to multiple host directories",
    )
    p_install.set_defaults(func=cmd_install)

    p_share = sub.add_parser("share", help="Plan sharing host-local skills into canonical")
    p_share.add_argument("skill", nargs="?", help="Host skill name")
    p_share.add_argument("--from", dest="from_host", required=True, help="Source host name")
    p_share.add_argument("--plan", action="store_true", help="Show candidate plan; change nothing")
    p_share.add_argument("--json", action="store_true", help="Print machine-readable JSON")
    share_state = p_share.add_mutually_exclusive_group()
    share_state.add_argument("--enable", action="store_true", help="Register shared skill as enabled")
    share_state.add_argument("--disable", action="store_true", help="Register shared skill as disabled")
    p_share.add_argument(
        "--force",
        action="store_true",
        help="Overwrite an existing different canonical skill",
    )
    p_share.add_argument(
        "--skip-audit",
        action="store_true",
        help="Explicitly bypass the audit gate before writing",
    )
    p_share.set_defaults(func=cmd_share)

    p_enable = sub.add_parser("enable", help="Enable a registered skill in the manifest")
    p_enable.add_argument("skill", help="Skill name")
    p_enable.set_defaults(func=cmd_enable)

    p_disable = sub.add_parser("disable", help="Disable a registered skill in the manifest")
    p_disable.add_argument("skill", help="Skill name")
    p_disable.set_defaults(func=cmd_disable)

    p_status = sub.add_parser("status", help="Show install status per skill and host")
    p_status.set_defaults(func=cmd_status)

    p_skills = sub.add_parser("skills", help="List registered canonical skills")
    p_skills.add_argument("--json", action="store_true", help="Print machine-readable JSON")
    p_skills.add_argument("--host", help="Show skills compatible with a host")
    state_filter = p_skills.add_mutually_exclusive_group()
    state_filter.add_argument("--enabled", action="store_true", help="Only enabled skills")
    state_filter.add_argument("--disabled", action="store_true", help="Only disabled skills")
    p_skills.add_argument(
        "--with-status",
        action="store_true",
        help="(deprecated) install status is always shown",
    )
    p_skills.set_defaults(func=cmd_skills)

    p_sync = sub.add_parser("sync", help="Update managed installs from canonical (copy mode)")
    p_sync.add_argument("skill", nargs="?", help="Skill name (default: enabled skills)")
    p_sync.add_argument("--host", help="Target host name, or 'all' (default: enabled targets)")
    p_sync.add_argument("--all", action="store_true", help="Include every registered skill")
    p_sync.add_argument("--check", action="store_true", help="Report drift only; change nothing")
    p_sync.add_argument(
        "--yes",
        action="store_true",
        help="Confirm writing to multiple host directories",
    )
    p_sync.add_argument(
        "--skip-audit",
        action="store_true",
        help="Explicitly bypass the audit gate before writing",
    )
    p_sync.set_defaults(func=cmd_sync)

    p_uninstall = sub.add_parser("uninstall", help="Remove managed installs")
    p_uninstall.add_argument("skill", nargs="?", help="Skill name")
    p_uninstall.add_argument("--host", help="Target host name, or 'all' (default: enabled targets)")
    p_uninstall.add_argument(
        "--yes",
        action="store_true",
        help="Confirm removing from multiple host directories",
    )
    p_uninstall.set_defaults(func=cmd_uninstall)

    p_import = sub.add_parser(
        "import", help="Import an external skill directory into canonical skills/"
    )
    p_import.add_argument("source", help="Path to a skill directory (contains SKILL.md)")
    p_import.add_argument(
        "--force", action="store_true", help="Overwrite an existing different canonical skill"
    )
    p_import.add_argument(
        "--skip-audit",
        action="store_true",
        help="Explicitly bypass the audit gate before writing",
    )
    p_import.set_defaults(func=cmd_import)

    p_data = sub.add_parser(
        "data-path",
        help="Print a skill's shared machine-local data directory",
    )
    p_data.add_argument("skill", help="Skill name")
    p_data.add_argument(
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
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main())
