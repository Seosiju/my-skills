from __future__ import annotations

import subprocess
from pathlib import Path

import pytest

from my_skills import cli


def test_init_registry_commits_scaffold_when_git_repo_already_exists(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    target = tmp_path / "my-agent-skills"
    target.mkdir()
    subprocess.run(["git", "init"], cwd=target, check=True, stdout=subprocess.DEVNULL)
    subprocess.run(
        ["git", "config", "user.email", "test@example.com"],
        cwd=target,
        check=True,
    )
    subprocess.run(
        ["git", "config", "user.name", "Test User"],
        cwd=target,
        check=True,
    )

    assert cli.main(["init-registry", str(target), "--no-defaults"]) == 0

    out = capsys.readouterr().out
    tracked = subprocess.run(
        ["git", "ls-files"],
        cwd=target,
        text=True,
        capture_output=True,
        check=True,
    ).stdout.splitlines()
    assert "git repository already exists; skipped git init" in out
    assert "Created initial registry commit" in out
    assert {".gitignore", "README.md", "my-skills.toml"} <= set(tracked)
