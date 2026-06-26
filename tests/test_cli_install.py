from pathlib import Path

from my_skills import cli


def _make_repo(tmp_path: Path) -> tuple[Path, Path]:
    """A repo whose only enabled target points at a tmp dir (never real HOME)."""
    target = tmp_path / "hosts" / "claude"
    others = "\n".join(
        f'[targets.{h}]\nenabled = false\npath = "{tmp_path / "hosts" / h}"\n'
        for h in ("codex", "hermes")
    )
    (tmp_path / "my-skills.toml").write_text(
        'schema_version = 1\nskills_root = "skills"\n\n'
        f'[targets.claude]\nenabled = true\nscope = "user"\npath = "{target}"\n\n'
        f"{others}\n"
        '[skills.alpha]\nenabled = true\nhosts = ["claude"]\n'
    )
    skill = tmp_path / "skills" / "alpha"
    skill.mkdir(parents=True)
    skill.joinpath("SKILL.md").write_text(
        "---\nname: alpha\ndescription: A valid skill for install tests.\n---\n\n# Alpha\n"
    )
    return tmp_path, target


def _prep(tmp_path, monkeypatch):
    repo, target = _make_repo(tmp_path)
    monkeypatch.chdir(repo)
    monkeypatch.setenv("XDG_STATE_HOME", str(tmp_path / "state"))
    return repo, target


def test_install_dry_run_writes_nothing(tmp_path, monkeypatch, capsys):
    repo, target = _prep(tmp_path, monkeypatch)
    assert cli.main(["install", "--dry-run"]) == 0
    assert "Dry run" in capsys.readouterr().out
    assert not (target / "alpha").exists()
    assert not (tmp_path / "state" / "my-skills" / "state.json").exists()


def test_install_creates_then_noop(tmp_path, monkeypatch, capsys):
    repo, target = _prep(tmp_path, monkeypatch)
    assert cli.main(["install"]) == 0
    assert (target / "alpha" / "SKILL.md").exists()
    capsys.readouterr()
    assert cli.main(["install"]) == 0
    assert "unchanged" in capsys.readouterr().out


def test_install_collision_blocks(tmp_path, monkeypatch, capsys):
    repo, target = _prep(tmp_path, monkeypatch)
    (target / "alpha").mkdir(parents=True)
    (target / "alpha" / "SKILL.md").write_text("foreign")
    assert cli.main(["install"]) == 1
    assert (target / "alpha" / "SKILL.md").read_text() == "foreign"


def test_install_invalid_skill_blocks(tmp_path, monkeypatch, capsys):
    repo, target = _prep(tmp_path, monkeypatch)
    (repo / "skills" / "alpha" / "SKILL.md").write_text("# no frontmatter\n")
    assert cli.main(["install"]) == 1
    assert not (target / "alpha").exists()


def test_link_mode_rejected(tmp_path, monkeypatch, capsys):
    _prep(tmp_path, monkeypatch)
    assert cli.main(["install", "--mode", "link"]) == 2


def test_status_missing_then_fresh(tmp_path, monkeypatch, capsys):
    repo, target = _prep(tmp_path, monkeypatch)
    cli.main(["status"])
    assert "MISSING" in capsys.readouterr().out
    cli.main(["install"])
    capsys.readouterr()
    cli.main(["status"])
    assert "FRESH" in capsys.readouterr().out


def test_uninstall_managed(tmp_path, monkeypatch, capsys):
    repo, target = _prep(tmp_path, monkeypatch)
    cli.main(["install"])
    capsys.readouterr()
    assert cli.main(["uninstall", "alpha"]) == 0
    assert not (target / "alpha").exists()
