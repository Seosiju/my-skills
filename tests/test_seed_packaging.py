from __future__ import annotations

import subprocess
import zipfile
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_SKILL_NAMES = ("cli-inventory", "personal-profile", "my-skills", "my-jira")


def test_wheel_contains_default_seed_skills(tmp_path: Path) -> None:
    out_dir = tmp_path / "dist"

    subprocess.run(
        ["uv", "build", "--wheel", "--out-dir", str(out_dir)],
        cwd=REPO_ROOT,
        check=True,
        capture_output=True,
        text=True,
    )

    [wheel] = out_dir.glob("my_skills-*.whl")
    with zipfile.ZipFile(wheel) as archive:
        packaged = set(archive.namelist())

    for skill_name in DEFAULT_SKILL_NAMES:
        assert f"my_skills/_defaults/skills/{skill_name}/SKILL.md" in packaged
