from __future__ import annotations

import argparse
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Final

from . import __version__
from .cli_runtime import cache_repo_root
from .defaults import DEFAULT_SEED_SKILLS, SeedUnavailable, seed_skills_dir

README_TITLE: Final = "Private Agent Skill Registry"
DEFAULT_REGISTRY: Final = "~/my-agent-skills"
SEED_HOSTS: Final = ("claude", "codex", "hermes")

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

README_BODY: Final = (
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
    target = _registry_target(args)
    overlaps = _existing_scaffold_files(target)
    if overlaps:
        print(
            f"error: registry scaffold already exists: {', '.join(overlaps)}",
            file=sys.stderr,
        )
        return 1

    target.mkdir(parents=True, exist_ok=True)
    skills = target / "skills"
    skills.mkdir()
    if args.with_defaults:
        try:
            _seed_default_skills(skills)
        except SeedUnavailable as exc:
            print(f"error: {exc}", file=sys.stderr)
            return 2
    (target / "my-skills.toml").write_text(
        _manifest(with_defaults=args.with_defaults), encoding="utf-8"
    )
    (target / ".gitignore").write_text(GITIGNORE, encoding="utf-8")
    (target / "README.md").write_text(_registry_readme(target), encoding="utf-8")
    cache_repo_root(target)
    if not args.no_git:
        _init_git(target)
    print(f"Created private skill registry at {target}")
    print(
        "Next: cd there, add skills/<name>/SKILL.md, "
        "then run `my-skills install --dry-run`."
    )
    return 0


def _registry_target(args: argparse.Namespace) -> Path:
    path = getattr(args, "path", None)
    if path:
        return Path(path).expanduser()
    if not sys.stdin.isatty():
        return Path(DEFAULT_REGISTRY).expanduser()
    return _prompt_registry_target()


def _prompt_registry_target() -> Path:
    while True:
        raw = input(f"Registry location [{DEFAULT_REGISTRY}]: ").strip()
        target = Path(raw or DEFAULT_REGISTRY).expanduser()
        if not (target / "my-skills.toml").exists():
            return target
        print(
            f"{target} already contains a my-skills registry. "
            "Enter a different path or use the existing registry."
        )


def _init_git(target: Path) -> None:
    if (target / ".git").exists():
        print("git repository already exists; skipped git init")
        return

    git = shutil.which("git")
    if git is None:
        print("git command not found; skipped git init")
        return

    init = subprocess.run(
        [git, "init"],
        cwd=target,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        check=False,
    )
    if init.returncode != 0:
        print("git init failed; skipped initial commit")
        return

    subprocess.run(
        [git, "add", "-A"],
        cwd=target,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        check=False,
    )
    commit = subprocess.run(
        [git, "commit", "-m", "chore: initialize my-skills registry"],
        cwd=target,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        check=False,
    )
    if commit.returncode == 0:
        print("Created initial registry commit")
    else:
        print("Initial git commit skipped; configure git user to commit")


def _seed_default_skills(target_skills: Path) -> None:
    source_skills = seed_skills_dir()
    for name, _enabled in DEFAULT_SEED_SKILLS:
        shutil.copytree(source_skills / name, target_skills / name)


def _manifest(*, with_defaults: bool) -> str:
    if not with_defaults:
        return MANIFEST
    return MANIFEST + _seed_skill_manifest()


def _seed_skill_manifest() -> str:
    lines: list[str] = []
    hosts = ", ".join(f'"{host}"' for host in SEED_HOSTS)
    for name, enabled in DEFAULT_SEED_SKILLS:
        lines.extend(
            (
                "",
                f"[skills.{name}]",
                f"enabled = {_toml_bool(enabled)}",
                f"hosts = [{hosts}]",
                'source_type = "builtin-seed"',
                f'source_revision = "{__version__}"',
            )
        )
    return "\n".join(lines) + "\n"


def _toml_bool(value: bool) -> str:
    return "true" if value else "false"


def _existing_scaffold_files(target: Path) -> tuple[str, ...]:
    paths = ("my-skills.toml", "skills", ".gitignore")
    overlaps = [name for name in paths if (target / name).exists()]
    readme = target / "README.md"
    if readme.exists() and _starter_readme_title(readme) is None:
        overlaps.append("README.md")
    return tuple(overlaps)


def _registry_readme(target: Path) -> str:
    readme = target / "README.md"
    title = README_TITLE
    if readme.exists():
        starter_title = _starter_readme_title(readme)
        if starter_title is not None:
            title = starter_title
    if title == README_TITLE:
        return f"# {title}\n\n{README_BODY}"
    return f"# {title}\n\n{README_TITLE}.\n\n{README_BODY}"


def _starter_readme_title(readme: Path) -> str | None:
    try:
        text = readme.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError):
        return None
    meaningful_lines = [line.strip() for line in text.splitlines() if line.strip()]
    if not meaningful_lines:
        return README_TITLE
    if len(meaningful_lines) != 1:
        return None
    [heading] = meaningful_lines
    if not heading.startswith("# "):
        return None
    title = heading[2:].strip()
    if not title:
        return README_TITLE
    return title
