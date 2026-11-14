from __future__ import annotations

import subprocess
import tarfile
import zipfile
from pathlib import Path

from my_skills.defaults import DEFAULT_SEED_FILES

REPO_ROOT = Path(__file__).resolve().parents[1]
SEED_PACKAGE_ROOT = "my_skills/_defaults/skills/"


def test_distributions_contain_defaults_without_retired_profile(tmp_path: Path) -> None:
    out_dir = tmp_path / "dist"

    subprocess.run(
        ["uv", "build", "--out-dir", str(out_dir)],
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
    assert not any("personal-profile" in Path(path).parts for path in packaged)
    assert not [
        path
        for path in seeded_paths
        if any(part.startswith(".") for part in Path(path).parts)
        or "__pycache__" in Path(path).parts
    ]

    [sdist] = out_dir.glob("my_skills-*.tar.gz")
    with tarfile.open(sdist) as archive:
        source_paths = archive.getnames()
    assert not any("personal-profile" in Path(path).parts for path in source_paths)
