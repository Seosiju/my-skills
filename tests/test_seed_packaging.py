from __future__ import annotations

import subprocess
import zipfile
from pathlib import Path

from my_skills.defaults import DEFAULT_SEED_FILES

REPO_ROOT = Path(__file__).resolve().parents[1]
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

    for seed_file in DEFAULT_SEED_FILES:
        assert f"{SEED_PACKAGE_ROOT}{seed_file}" in packaged

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
