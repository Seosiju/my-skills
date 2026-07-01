import json
from pathlib import Path

import pytest

from my_skills import cli


def _make_repo(tmp_path: Path, skill_name: str = "good-skill", body: str | None = None) -> Path:
    (tmp_path / "my-skills.toml").write_text('schema_version = 1\nskills_root = "skills"\n')
    skill_dir = tmp_path / "skills" / skill_name
    skill_dir.mkdir(parents=True)
    if body is None:
        body = (
            f"---\nname: {skill_name}\n"
            f"description: A valid skill used in CLI tests.\n---\n\n# {skill_name}\n"
        )
    (skill_dir / "SKILL.md").write_text(body)
    return tmp_path


def test_validate_good_exit_zero(tmp_path, monkeypatch, capsys):
    monkeypatch.chdir(_make_repo(tmp_path))
    rc = cli.main(["validate"])
    assert rc == 0
    assert "[OK]" in capsys.readouterr().out


def test_validate_malformed_exit_nonzero(tmp_path, monkeypatch):
    monkeypatch.chdir(_make_repo(tmp_path, body="# no frontmatter here\n"))
    assert cli.main(["validate"]) == 1


def test_doctor_exit_zero(tmp_path, monkeypatch, capsys):
    monkeypatch.chdir(_make_repo(tmp_path))
    rc = cli.main(["doctor"])
    out = capsys.readouterr().out
    assert rc == 0
    assert "Manifest: valid" in out
    assert "Hosts:" in out


def test_no_command_prints_help(capsys):
    assert cli.main([]) == 0
    assert "usage" in capsys.readouterr().out.lower()


def test_bootstrap_help_marks_contributor_only(capsys: pytest.CaptureFixture[str]) -> None:
    with pytest.raises(SystemExit) as excinfo:
        _ = cli.main(["bootstrap", "-h"])

    assert excinfo.value.code == 0
    assert "(contributor/dev only)" in capsys.readouterr().out


def test_init_registry_scaffolds_private_registry(tmp_path, capsys):
    target = tmp_path / "my-agent-skills"

    rc = cli.main(["init-registry", str(target)])

    out = capsys.readouterr().out
    assert rc == 0
    assert "Created private skill registry" in out
    assert (target / "my-skills.toml").is_file()
    assert (target / "skills").is_dir()
    assert (target / "README.md").is_file()
    assert (target / ".gitignore").read_text(encoding="utf-8") == (
        "my-skills.local.toml\nlocal/\n"
    )
    manifest = (target / "my-skills.toml").read_text(encoding="utf-8")
    assert 'skills_root = "skills"' in manifest
    assert "[targets.claude]" in manifest
    assert "[targets.codex]" in manifest
    assert "[targets.hermes]" in manifest
    readme = (target / "README.md").read_text(encoding="utf-8")
    assert "Private Agent Skill Registry" in readme
    assert "uv tool install git+https://github.com/Seosiju/my-skills.git" in readme


def test_init_registry_accepts_github_starter_readme(tmp_path, capsys):
    target = tmp_path / "my-agent-skills"
    target.mkdir()
    (target / "README.md").write_text("# my-agent-skills\n", encoding="utf-8")

    rc = cli.main(["init-registry", str(target)])

    out = capsys.readouterr().out
    assert rc == 0
    assert "Created private skill registry" in out
    assert (target / "my-skills.toml").is_file()
    assert (target / "skills").is_dir()
    readme = (target / "README.md").read_text(encoding="utf-8")
    assert readme.startswith("# my-agent-skills\n")
    assert "Private Agent Skill Registry" in readme
    assert "my-skills install --dry-run" in readme


def test_init_registry_refuses_existing_substantial_readme(tmp_path, capsys):
    target = tmp_path / "existing"
    target.mkdir()
    original_readme = "# Existing Project\n\nDo not replace this documentation.\n"
    (target / "README.md").write_text(original_readme, encoding="utf-8")

    rc = cli.main(["init-registry", str(target)])

    assert rc == 1
    assert "README.md" in capsys.readouterr().err
    assert (target / "README.md").read_text(encoding="utf-8") == original_readme


def test_init_registry_refuses_existing_manifest(tmp_path, capsys):
    target = tmp_path / "existing"
    target.mkdir()
    (target / "my-skills.toml").write_text("keep me\n", encoding="utf-8")

    rc = cli.main(["init-registry", str(target)])

    assert rc == 1
    assert "already exists" in capsys.readouterr().err
    assert (target / "my-skills.toml").read_text(encoding="utf-8") == "keep me\n"


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


def test_skills_table_shows_per_host_status_columns_without_summary(
    tmp_path, monkeypatch, capsys
):
    monkeypatch.chdir(_make_skills_repo(tmp_path))
    monkeypatch.setenv("XDG_STATE_HOME", str(tmp_path / "state"))

    rc = cli.main(["skills"])

    out = capsys.readouterr().out
    assert rc == 0
    # header: skill name, enabled flag, one column per enabled host target
    assert "SKILL" in out
    assert "ENABLED" in out
    assert "CLAUDE" in out
    assert "CODEX" in out
    # summary/description is no longer rendered in the list view
    assert "Summary" not in out
    assert "Alpha summary from frontmatter." not in out
    # every skill is listed
    assert "alpha" in out
    assert "beta" in out
    assert "gamma" in out
    # beta targets only codex, so its claude cell is "-" (not targeted)
    assert "-" in out


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


def test_skills_enabled_and_disabled_are_mutually_exclusive():
    with pytest.raises(SystemExit) as excinfo:
        cli.main(["skills", "--enabled", "--disabled"])

    assert excinfo.value.code == 2
