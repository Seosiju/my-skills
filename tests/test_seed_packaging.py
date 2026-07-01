from __future__ import annotations

import subprocess
import zipfile
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_SKILL_NAMES = ("cli-inventory", "personal-profile", "my-skills", "my-jira")
SEED_PACKAGE_ROOT = "my_skills/_defaults/skills/"


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

    seeded_paths = [
        name.removeprefix(SEED_PACKAGE_ROOT)
        for name in packaged
        if name.startswith(SEED_PACKAGE_ROOT)
    ]
    assert seeded_paths
    assert not [
        path
        for path in seeded_paths
        if any(part.startswith(".") for part in Path(path).parts)
        or "__pycache__" in Path(path).parts
    ]
