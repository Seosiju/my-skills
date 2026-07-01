from __future__ import annotations

import json
import os
import subprocess
import tomllib
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_ENABLED_SKILLS = {"cli-inventory", "personal-profile", "my-skills"}
DEFAULT_DISABLED_SKILLS = {"my-jira"}


def _cold_env(tmp_path: Path) -> tuple[dict[str, str], Path, Path]:
    home = tmp_path / "home"
    work = tmp_path / "work"
    home.mkdir()
    work.mkdir()

    env = os.environ.copy()
    env.pop("MY_SKILLS_ROOT", None)
    env["HOME"] = str(home)
    env["XDG_CONFIG_HOME"] = str(home / ".config")
    env["XDG_STATE_HOME"] = str(home / ".local" / "state")
    return env, home, work


def _run_cli(args: list[str], *, env: dict[str, str], cwd: Path) -> subprocess.CompletedProcess[str]:
    result = subprocess.run(
        ["uv", "--project", str(REPO_ROOT), "run", "--frozen", "my-skills", *args],
        cwd=cwd,
        env=env,
        text=True,
        capture_output=True,
        check=False,
    )
    assert result.returncode == 0, (
        f"my-skills {' '.join(args)} failed with {result.returncode}\n"
        f"stdout:\n{result.stdout}\n"
        f"stderr:\n{result.stderr}"
    )
    return result


def test_seeded_registry_cold_cli_flow_installs_claude_host(tmp_path: Path) -> None:
    env, home, work = _cold_env(tmp_path)
    registry = home / "reg"

    _run_cli(["init-registry", str(registry), "--no-git"], env=env, cwd=work)

    manifest = tomllib.loads((registry / "my-skills.toml").read_text(encoding="utf-8"))
    assert set(manifest["skills"]) == DEFAULT_ENABLED_SKILLS | DEFAULT_DISABLED_SKILLS
    for skill in DEFAULT_ENABLED_SKILLS | DEFAULT_DISABLED_SKILLS:
        assert (registry / "skills" / skill / "SKILL.md").is_file()
    seeded_paths = [
        path.relative_to(registry / "skills")
        for path in (registry / "skills").rglob("*")
    ]
    assert not [
        path
        for path in seeded_paths
        if any(part.startswith(".") for part in path.parts)
        or "__pycache__" in path.parts
    ]

    skills = json.loads(_run_cli(["skills", "--json"], env=env, cwd=work).stdout)
    assert {row["name"] for row in skills["skills"]} == DEFAULT_ENABLED_SKILLS | DEFAULT_DISABLED_SKILLS

    dry_run = json.loads(
        _run_cli(["install", "--dry-run", "--json"], env=env, cwd=work).stdout
    )
    planned_skills = {action["skill"] for action in dry_run["actions"]}
    assert DEFAULT_ENABLED_SKILLS <= planned_skills
    assert "my-jira" not in planned_skills

    _run_cli(["install", "--host", "claude", "--yes"], env=env, cwd=work)

    assert (home / ".claude" / "skills" / "my-skills" / "SKILL.md").is_file()
    assert (home / ".claude" / "skills" / "cli-inventory" / "SKILL.md").is_file()
    assert not (home / ".claude" / "skills" / "my-jira").exists()


def test_no_defaults_cold_cli_flow_creates_empty_registry(tmp_path: Path) -> None:
    env, home, work = _cold_env(tmp_path)
    registry = home / "empty-reg"

    _run_cli(
        ["init-registry", str(registry), "--no-defaults", "--no-git"],
        env=env,
        cwd=work,
    )

    manifest = tomllib.loads((registry / "my-skills.toml").read_text(encoding="utf-8"))
    assert "skills" not in manifest
    assert not any((registry / "skills").iterdir())

    skills = json.loads(_run_cli(["skills", "--json"], env=env, cwd=work).stdout)
    assert skills["skills"] == []
