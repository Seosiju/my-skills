"""Temporary-HOME integration test (plan section 16.3 subset, Phase 2 evidence)."""

import textwrap
from pathlib import Path

from my_skills import cli

MANIFEST = """
schema_version = 1
skills_root = "skills"

[targets.claude]
enabled = true
path = "~/.claude/skills"

[targets.codex]
enabled = false
path = "~/.agents/skills"

[targets.gemini]
enabled = false
path = "~/.gemini/skills"

[targets.hermes]
enabled = false
path = "~/.hermes/skills"

[skills.alpha]
enabled = true
hosts = ["claude"]
"""


def _setup(tmp_path, monkeypatch) -> tuple[Path, Path]:
    home = tmp_path / "home"
    home.mkdir()
    monkeypatch.setenv("HOME", str(home))
    monkeypatch.setenv("XDG_STATE_HOME", str(home / ".local" / "state"))

    repo = tmp_path / "repo"
    (repo / "skills" / "alpha").mkdir(parents=True)
    (repo / "skills" / "alpha" / "SKILL.md").write_text(
        "---\nname: alpha\ndescription: Canonical skill for the integration test.\n---\n\n# Alpha\nbody\n"
    )
    (repo / "my-skills.toml").write_text(textwrap.dedent(MANIFEST))
    monkeypatch.chdir(repo)
    return home, repo


def test_install_then_repeat_is_noop(tmp_path, monkeypatch, capsys):
    home, repo = _setup(tmp_path, monkeypatch)
    dest = home / ".claude" / "skills" / "alpha"

    assert cli.main(["install", "--all"]) == 0
    content = (dest / "SKILL.md").read_text()
    assert content.splitlines()[0] == "---"
    assert "name: alpha" in content
    capsys.readouterr()

    # Completion evidence #3: a repeated install changes nothing.
    assert cli.main(["install", "--all"]) == 0
    assert "unchanged" in capsys.readouterr().out


def test_uninstall_preserves_unmanaged(tmp_path, monkeypatch, capsys):
    home, repo = _setup(tmp_path, monkeypatch)
    skills_dir = home / ".claude" / "skills"

    cli.main(["install", "--all"])
    capsys.readouterr()

    unmanaged = skills_dir / "hand-made"
    unmanaged.mkdir(parents=True)
    (unmanaged / "SKILL.md").write_text("mine")

    cli.main(["uninstall", "alpha"])
    # Completion evidence #2: managed removed, unmanaged sibling preserved.
    assert not (skills_dir / "alpha").exists()
    assert (unmanaged / "SKILL.md").read_text() == "mine"


def test_unmanaged_collision_is_blocked(tmp_path, monkeypatch, capsys):
    home, repo = _setup(tmp_path, monkeypatch)
    dest = home / ".claude" / "skills" / "alpha"
    dest.mkdir(parents=True)
    (dest / "SKILL.md").write_text("foreign-unmanaged")

    assert cli.main(["install", "--all"]) == 1
    assert (dest / "SKILL.md").read_text() == "foreign-unmanaged"
