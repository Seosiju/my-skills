from __future__ import annotations

from pathlib import Path

import pytest

from my_skills import cli
from my_skills.defaults import DEFAULT_SEED_SKILLS


def _make_repo(
    tmp_path: Path, skill_name: str = "good-skill", body: str | None = None
) -> Path:
    _ = (tmp_path / "my-skills.toml").write_text(
        'schema_version = 1\nskills_root = "skills"\n'
    )
    skill_dir = tmp_path / "skills" / skill_name
    skill_dir.mkdir(parents=True)
    if body is None:
        body = (
            f"---\nname: {skill_name}\n"
            f"description: A valid skill used in CLI tests.\n---\n\n# {skill_name}\n"
        )
    _ = (skill_dir / "SKILL.md").write_text(body)
    return tmp_path


def test_validate_good_exit_zero(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
) -> None:
    monkeypatch.chdir(_make_repo(tmp_path))
    rc = cli.main(["validate"])
    assert rc == 0
    assert "[OK]" in capsys.readouterr().out


def test_validate_malformed_exit_nonzero(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.chdir(_make_repo(tmp_path, body="# no frontmatter here\n"))
    assert cli.main(["validate"]) == 1


def test_doctor_exit_zero(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
) -> None:
    monkeypatch.chdir(_make_repo(tmp_path))
    rc = cli.main(["doctor", "--no-update-check"])
    out = capsys.readouterr().out
    assert rc == 0
    assert "Manifest: valid" in out
    assert "Hosts:" in out


def test_no_command_prints_help(capsys: pytest.CaptureFixture[str]) -> None:
    assert cli.main([]) == 0
    assert "usage" in capsys.readouterr().out.lower()


def test_bootstrap_help_marks_contributor_only(capsys: pytest.CaptureFixture[str]) -> None:
    with pytest.raises(SystemExit) as excinfo:
        _ = cli.main(["bootstrap", "-h"])

    assert excinfo.value.code == 0
    assert "(contributor/dev only)" in capsys.readouterr().out


def test_init_registry_scaffolds_private_registry(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    target = tmp_path / "my-agent-skills"

    rc = cli.main(["init-registry", str(target)])

    out = capsys.readouterr().out
    assert rc == 0
    assert "Created private skill registry" in out
    assert (target / "my-skills.toml").is_file()
    assert (target / "skills").is_dir()
    assert (target / "README.md").is_file()
    gitignore = (target / ".gitignore").read_text(encoding="utf-8")
    assert "my-skills.local.toml\n" in gitignore
    assert "local/\n" in gitignore
    assert ".omc/\n" in gitignore
    manifest = (target / "my-skills.toml").read_text(encoding="utf-8")
    assert 'skills_root = "skills"' in manifest
    assert "[targets.claude]" in manifest
    assert "[targets.codex]" in manifest
    assert "[targets.hermes]" in manifest
    readme = (target / "README.md").read_text(encoding="utf-8")
    assert "Private Agent Skill Registry" in readme
    assert "uv tool install git+https://github.com/Seosiju/my-skills.git" in readme


def test_init_registry_seeds_default_skills(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    target = tmp_path / "my-agent-skills"

    rc = cli.main(["init-registry", str(target)])

    assert rc == 0
    _ = capsys.readouterr()
    for name, _enabled in DEFAULT_SEED_SKILLS:
        assert (target / "skills" / name / "SKILL.md").is_file()
    assert (target / "skills" / "my-jira" / "config.example.json").is_file()
    assert not (target / "skills" / "my-jira" / "config.json").exists()


def test_init_registry_no_defaults_keeps_empty_skills_dir(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    target = tmp_path / "my-agent-skills"

    rc = cli.main(["init-registry", str(target), "--no-defaults"])

    assert rc == 0
    _ = capsys.readouterr()
    assert list((target / "skills").iterdir()) == []


def test_init_registry_accepts_github_starter_readme(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    target = tmp_path / "my-agent-skills"
    target.mkdir()
    _ = (target / "README.md").write_text("# my-agent-skills\n", encoding="utf-8")

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


def test_init_registry_refuses_existing_substantial_readme(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    target = tmp_path / "existing"
    target.mkdir()
    original_readme = "# Existing Project\n\nDo not replace this documentation.\n"
    _ = (target / "README.md").write_text(original_readme, encoding="utf-8")

    rc = cli.main(["init-registry", str(target)])

    assert rc == 1
    assert "README.md" in capsys.readouterr().err
    assert (target / "README.md").read_text(encoding="utf-8") == original_readme


def test_init_registry_refuses_existing_manifest(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    target = tmp_path / "existing"
    target.mkdir()
    _ = (target / "my-skills.toml").write_text("keep me\n", encoding="utf-8")

    rc = cli.main(["init-registry", str(target)])

    assert rc == 1
    assert "already exists" in capsys.readouterr().err
    assert (target / "my-skills.toml").read_text(encoding="utf-8") == "keep me\n"
