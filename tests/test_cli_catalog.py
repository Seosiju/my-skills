import json
from pathlib import Path

import pytest

from my_skills import cli


def _make_skills_repo(tmp_path: Path) -> Path:
    manifest = """schema_version = 1
skills_root = "skills"

[targets.claude]
enabled = true
scope = "user"
path = "./installed/claude"

[targets.codex]
enabled = true
scope = "user"
path = "./installed/codex"

[skills.alpha]
enabled = true
hosts = ["claude", "codex"]

[skills.beta]
enabled = false
hosts = ["codex"]

[skills.gamma]
enabled = true
hosts = []
"""
    (tmp_path / "my-skills.toml").write_text(manifest)
    for name, description in (
        ("alpha", "Alpha summary from frontmatter."),
        ("beta", "Beta summary from frontmatter."),
        ("gamma", "Gamma summary from frontmatter."),
    ):
        skill_dir = tmp_path / "skills" / name
        skill_dir.mkdir(parents=True)
        (skill_dir / "SKILL.md").write_text(
            f"---\nname: {name}\ndescription: {description}\n---\n\n# {name}\n"
        )
    return tmp_path


def _add_invalid_registered_skill(repo: Path) -> None:
    with (repo / "my-skills.toml").open("a", encoding="utf-8") as fh:
        fh.write('\n[skills.broken]\nenabled = true\nhosts = ["claude"]\n')


def test_skills_table_shows_per_host_status_columns_without_summary(
    tmp_path, monkeypatch, capsys
):
    monkeypatch.chdir(_make_skills_repo(tmp_path))
    monkeypatch.setenv("XDG_STATE_HOME", str(tmp_path / "state"))

    rc = cli.main(["skills"])

    out = capsys.readouterr().out
    assert rc == 0
    assert "SKILL" in out
    assert "ENABLED" in out
    assert "CLAUDE" in out
    assert "CODEX" in out
    assert "Summary" not in out
    assert "Alpha summary from frontmatter." not in out
    assert "alpha" in out
    assert "beta" in out
    assert "gamma" in out
    assert "-" in out


def test_skills_table_includes_invalid_rows_without_failing(
    tmp_path,
    monkeypatch,
    capsys,
):
    repo = _make_skills_repo(tmp_path)
    _add_invalid_registered_skill(repo)
    monkeypatch.chdir(repo)
    monkeypatch.setenv("XDG_STATE_HOME", str(tmp_path / "state"))

    rc = cli.main(["skills"])

    captured = capsys.readouterr()
    assert rc == 0
    assert captured.err == ""
    assert "alpha" in captured.out
    assert "beta" in captured.out
    assert "gamma" in captured.out
    assert "broken (invalid)" in captured.out


def test_skills_json_filters_by_host_and_enabled_state(tmp_path, monkeypatch, capsys):
    monkeypatch.chdir(_make_skills_repo(tmp_path))
    monkeypatch.setenv("XDG_STATE_HOME", str(tmp_path / "state"))

    rc = cli.main(["skills", "--json", "--host", "codex", "--enabled"])

    payload = json.loads(capsys.readouterr().out)
    assert rc == 0
    assert payload == {
        "skills": [
            {
                "name": "alpha",
                "enabled": True,
                "hosts": ["claude", "codex"],
                "description": "Alpha summary from frontmatter.",
                "path": "skills/alpha",
                "status": {"codex": "MISSING"},
            },
            {
                "name": "gamma",
                "enabled": True,
                "hosts": [],
                "description": "Gamma summary from frontmatter.",
                "path": "skills/gamma",
                "status": {"codex": "MISSING"},
            },
        ]
    }


def test_skills_json_includes_invalid_row_error(
    tmp_path,
    monkeypatch,
    capsys,
):
    repo = _make_skills_repo(tmp_path)
    _add_invalid_registered_skill(repo)
    monkeypatch.chdir(repo)
    monkeypatch.setenv("XDG_STATE_HOME", str(tmp_path / "state"))

    rc = cli.main(["skills", "--json", "--host", "claude", "--enabled"])

    payload = json.loads(capsys.readouterr().out)
    broken = next(row for row in payload["skills"] if row["name"] == "broken")
    assert rc == 0
    assert broken["status"] == {"claude": "MISSING"}
    assert broken["description"].startswith("invalid: ")
    assert "SKILL.md" in broken["error"]
    assert [row["name"] for row in payload["skills"]] == ["alpha", "broken", "gamma"]


def test_skills_with_status_uses_selected_host_only(
    tmp_path, monkeypatch, capsys
):
    monkeypatch.chdir(_make_skills_repo(tmp_path))
    monkeypatch.setenv("XDG_STATE_HOME", str(tmp_path / "state"))

    rc = cli.main(["skills", "--json", "--host", "codex", "--disabled", "--with-status"])

    payload = json.loads(capsys.readouterr().out)
    assert rc == 0
    assert {row["name"]: row["status"] for row in payload["skills"]} == {
        "beta": {"codex": "MISSING"},
    }


def test_skills_rejects_unknown_host(tmp_path, monkeypatch, capsys):
    monkeypatch.chdir(_make_skills_repo(tmp_path))

    rc = cli.main(["skills", "--host", "ghost"])

    err = capsys.readouterr().err
    assert rc == 2
    assert "unknown host: ghost" in err


def test_status_reports_newer_state_schema_as_usage_error(
    tmp_path,
    monkeypatch,
    capsys,
):
    monkeypatch.chdir(_make_skills_repo(tmp_path))
    monkeypatch.setenv("XDG_STATE_HOME", str(tmp_path / "state"))
    state_path = tmp_path / "state" / "my-skills" / "state.json"
    state_path.parent.mkdir(parents=True)
    state_path.write_text('{"schema_version": 2, "installs": []}\n', encoding="utf-8")

    rc = cli.main(["status"])

    err = capsys.readouterr().err
    assert rc == 2
    assert "newer my-skills" in err
    assert str(state_path) in err


def test_skills_reports_newer_state_schema_as_usage_error(
    tmp_path,
    monkeypatch,
    capsys,
):
    monkeypatch.chdir(_make_skills_repo(tmp_path))
    monkeypatch.setenv("XDG_STATE_HOME", str(tmp_path / "state"))
    state_path = tmp_path / "state" / "my-skills" / "state.json"
    state_path.parent.mkdir(parents=True)
    state_path.write_text('{"schema_version": 2, "installs": []}\n', encoding="utf-8")

    rc = cli.main(["skills"])

    err = capsys.readouterr().err
    assert rc == 2
    assert "newer my-skills" in err
    assert str(state_path) in err


def test_skills_enabled_and_disabled_are_mutually_exclusive():
    with pytest.raises(SystemExit) as excinfo:
        cli.main(["skills", "--enabled", "--disabled"])

    assert excinfo.value.code == 2
