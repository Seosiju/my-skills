from pathlib import Path

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
