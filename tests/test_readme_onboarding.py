from __future__ import annotations

from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]


def _section(text: str, heading: str) -> str:
    start = text.index(heading)
    next_heading = text.find("\n## ", start + len(heading))
    if next_heading == -1:
        return text[start:]
    return text[start:next_heading]


def test_readmes_use_init_registry_as_first_run_front_door() -> None:
    english = (REPO_ROOT / "README.md").read_text(encoding="utf-8")
    korean = (REPO_ROOT / "README.ko.md").read_text(encoding="utf-8")

    english_quick_start = _section(english, "## Quick start")
    korean_quick_start = _section(korean, "## 빠른 시작")

    for section in (english_quick_start, korean_quick_start):
        assert "uv tool install git+https://github.com/Seosiju/my-skills.git" in section
        assert "my-skills init-registry" in section
        assert "my-skills install --dry-run" in section
        assert "my-skills bootstrap" not in section
        assert "git clone https://github.com/Seosiju/my-skills.git" not in section
