from __future__ import annotations

import argparse
import sys
from pathlib import Path
from typing import Final

MANIFEST: Final = (
    'schema_version = 1\n'
    'skills_root = "skills"\n'
    "\n"
    "[defaults]\n"
    'install_mode = "copy"\n'
    'collision = "error"\n'
    "verify_after_install = true\n"
    "\n"
    "[targets.claude]\n"
    "enabled = true\n"
    'scope = "user"\n'
    'path = "~/.claude/skills"\n'
    "\n"
    "[targets.codex]\n"
    "enabled = true\n"
    'scope = "user"\n'
    'path = "~/.agents/skills"\n'
    "\n"
    "[targets.hermes]\n"
    "enabled = true\n"
    'scope = "user"\n'
    'path = "~/.hermes/skills"\n'
)

GITIGNORE: Final = "my-skills.local.toml\nlocal/\n"

README: Final = (
    "# Private Agent Skill Registry\n"
    "\n"
    "This repository is your canonical source for private Agent Skills.\n"
    "\n"
    "Install the public `my-skills` CLI:\n"
    "\n"
    "```bash\n"
    "uv tool install git+https://github.com/Seosiju/my-skills.git\n"
    "```\n"
    "\n"
    "Add skills under `skills/<name>/SKILL.md`, then preview and install them:\n"
    "\n"
    "```bash\n"
    "my-skills skills\n"
    "my-skills install --dry-run\n"
    "my-skills install\n"
    "```\n"
    "\n"
    "Keep secrets, account data, personal memory, and machine-specific overrides out\n"
    "of git. Store local data with:\n"
    "\n"
    "```bash\n"
    "my-skills data-path <skill> --create\n"
    "```\n"
    "\n"
    "Use `my-skills.local.toml` for machine-specific manifest overrides.\n"
)


def cmd_init_registry(args: argparse.Namespace) -> int:
    target = Path(args.path).expanduser()
    overlaps = _existing_scaffold_files(target)
    if overlaps:
        print(
            f"error: registry scaffold already exists: {', '.join(overlaps)}",
            file=sys.stderr,
        )
        return 1

    target.mkdir(parents=True, exist_ok=True)
    (target / "skills").mkdir()
    (target / "my-skills.toml").write_text(MANIFEST, encoding="utf-8")
    (target / ".gitignore").write_text(GITIGNORE, encoding="utf-8")
    (target / "README.md").write_text(README, encoding="utf-8")
    print(f"Created private skill registry at {target}")
    print(
        "Next: cd there, add skills/<name>/SKILL.md, "
        "then run `my-skills install --dry-run`."
    )
    return 0


def _existing_scaffold_files(target: Path) -> tuple[str, ...]:
    paths = ("my-skills.toml", "skills", ".gitignore", "README.md")
    return tuple(name for name in paths if (target / name).exists())
