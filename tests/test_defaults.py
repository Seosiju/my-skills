from __future__ import annotations

from pathlib import Path

import pytest

from my_skills import defaults


EXPECTED_SEED_SKILLS = (
    ("cli-inventory", True),
    ("personal-profile", True),
    ("my-skills", True),
    ("my-jira", False),
)


def _write_seed_tree(root: Path) -> None:
    for name, _enabled in EXPECTED_SEED_SKILLS:
        skill_dir = root / name
        skill_dir.mkdir(parents=True)
        (skill_dir / "SKILL.md").write_text(f"# {name}\n", encoding="utf-8")


def test_default_seed_skills_are_defined_once() -> None:
    assert defaults.DEFAULT_SEED_SKILLS == EXPECTED_SEED_SKILLS


def test_seed_skills_dir_prefers_packaged_defaults(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    packaged = tmp_path / "packaged"
    repo = tmp_path / "repo"
    _write_seed_tree(packaged)
    _write_seed_tree(repo / "skills")

    monkeypatch.setattr(defaults, "_packaged_seed_dir", lambda: packaged)
    monkeypatch.setattr(defaults, "package_repo_root", lambda: repo)

    assert defaults.seed_skills_dir() == packaged


def test_seed_skills_dir_falls_back_to_repo_skills(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    repo = tmp_path / "repo"
    source_skills = repo / "skills"
    _write_seed_tree(source_skills)

    monkeypatch.setattr(defaults, "_packaged_seed_dir", lambda: None)
    monkeypatch.setattr(defaults, "package_repo_root", lambda: repo)

    assert defaults.seed_skills_dir() == source_skills


def test_seed_skills_dir_raises_when_defaults_are_unavailable(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setattr(defaults, "_packaged_seed_dir", lambda: None)
    monkeypatch.setattr(defaults, "package_repo_root", lambda: tmp_path / "missing")

    with pytest.raises(defaults.SeedUnavailable):
        defaults.seed_skills_dir()
