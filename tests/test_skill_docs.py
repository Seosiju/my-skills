from __future__ import annotations

from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]


def test_my_skills_skill_documents_seed_registry_front_door() -> None:
    text = (REPO_ROOT / "skills/my-skills/SKILL.md").read_text(encoding="utf-8")

    assert "setting up or creating a registry" in text
    assert "default `~/my-agent-skills`" in text
    assert "my-skills init-registry" in text
    assert "--no-defaults" in text
    assert "--no-git" in text
    assert "`bootstrap` is for **contributors/maintainers**" in text
