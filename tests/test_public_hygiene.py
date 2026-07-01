from __future__ import annotations

import subprocess
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
TEXT_SUFFIXES = {".md", ".json", ".toml", ".py", ".yaml", ".yml", ".txt", ""}
CLI_INVENTORY_SCRATCH = {
    "skills/cli-inventory/cvelist",
    "skills/cli-inventory/cvelist-our",
    "skills/cli-inventory/cvelist.diff",
    "skills/cli-inventory/n",
    "skills/cli-inventory/patcheck.tmp",
}
ROADMAP_PATH = REPO_ROOT / "docs/2026-06-30-my-skills-open-source-roadmap.md"


def _tracked_files() -> list[str]:
    proc = subprocess.run(
        ["git", "ls-files"],
        cwd=REPO_ROOT,
        check=True,
        capture_output=True,
        text=True,
    )
    return [line for line in proc.stdout.splitlines() if line]


def _tracked_text() -> dict[str, str]:
    text_by_path: dict[str, str] = {}
    for rel in _tracked_files():
        path = REPO_ROOT / rel
        if path.suffix not in TEXT_SUFFIXES:
            continue
        text_by_path[rel] = path.read_text(encoding="utf-8")
    return text_by_path


def test_tracked_files_do_not_contain_actual_private_jira_values() -> None:
    private_values = [
        "".join(("hermes", "forjs")),
        "".join(("e0ffe", "317", "-5557-43b4-a551-466ff24275a2")),
        "".join(("ideal", "innov39", "@", "gmail", ".com")),
        "".join(("712", "020:", "076771c9-2a42-4120-877f-2b10b68c08bd")),
        "".join(("jun", "seon")),
    ]

    leaks: list[str] = []
    for rel, text in _tracked_text().items():
        leaks.extend(f"{rel}: {value}" for value in private_values if value in text)

    assert leaks == []


def test_tracked_files_do_not_contain_actual_machine_home_path() -> None:
    private_home = "".join(("/Users/", "snu", ".sim"))
    leaks = [rel for rel, text in _tracked_text().items() if private_home in text]

    assert leaks == []


def test_my_jira_uses_example_config_not_tracked_private_config() -> None:
    tracked = set(_tracked_files())

    assert "skills/my-jira/config.json" not in tracked
    assert "skills/my-jira/config.example.json" in tracked


def test_cli_inventory_scratch_files_are_not_tracked() -> None:
    tracked = set(_tracked_files())

    assert tracked.isdisjoint(CLI_INVENTORY_SCRATCH)


def test_roadmap_links_seed_design() -> None:
    roadmap = ROADMAP_PATH.read_text(encoding="utf-8")

    assert "docs/2026-07-01-default-skills-seed-design.md" in roadmap
    assert "docs/2026-07-01-seed-implementation-spec.md" in roadmap
