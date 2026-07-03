from pathlib import Path

from my_skills import cli
from my_skills import install_commands
from my_skills.state import State


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


def _make_multi_host_repo(tmp_path: Path) -> tuple[Path, Path, Path]:
    claude = tmp_path / "hosts" / "claude"
    codex = tmp_path / "hosts" / "codex"
    (tmp_path / "my-skills.toml").write_text(
        'schema_version = 1\nskills_root = "skills"\n\n'
        f'[targets.claude]\nenabled = true\nscope = "user"\npath = "{claude}"\n\n'
        f'[targets.codex]\nenabled = true\nscope = "user"\npath = "{codex}"\n\n'
        f'[targets.hermes]\nenabled = false\nscope = "user"\npath = "{tmp_path / "hosts" / "hermes"}"\n\n'
        '[skills.alpha]\nenabled = true\nhosts = ["claude", "codex"]\n'
    )
    skill = tmp_path / "skills" / "alpha"
    skill.mkdir(parents=True)
    skill.joinpath("SKILL.md").write_text(
        "---\nname: alpha\ndescription: A valid multi-host skill.\n---\n\n# Alpha\n"
    )
    return tmp_path, claude, codex


def _make_two_skill_repo(tmp_path: Path) -> tuple[Path, Path]:
    target = tmp_path / "hosts" / "claude"
    (tmp_path / "my-skills.toml").write_text(
        'schema_version = 1\nskills_root = "skills"\n\n'
        f'[targets.claude]\nenabled = true\nscope = "user"\npath = "{target}"\n\n'
        f'[targets.codex]\nenabled = false\nscope = "user"\npath = "{tmp_path / "hosts" / "codex"}"\n\n'
        f'[targets.hermes]\nenabled = false\nscope = "user"\npath = "{tmp_path / "hosts" / "hermes"}"\n\n'
        '[skills.alpha]\nenabled = true\nhosts = ["claude"]\n\n'
        '[skills.beta]\nenabled = true\nhosts = ["claude"]\n'
    )
    for name in ("alpha", "beta"):
        skill = tmp_path / "skills" / name
        skill.mkdir(parents=True)
        skill.joinpath("SKILL.md").write_text(
            f"---\nname: {name}\ndescription: {name} install test skill.\n---\n\n# {name}\n"
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


def test_install_multi_host_requires_yes_after_dry_run(tmp_path, monkeypatch, capsys):
    repo, claude, codex = _make_multi_host_repo(tmp_path)
    monkeypatch.chdir(repo)
    monkeypatch.setenv("XDG_STATE_HOME", str(tmp_path / "state"))

    assert cli.main(["install", "alpha", "--host", "all", "--dry-run"]) == 0
    assert "CREATE" in capsys.readouterr().out

    assert cli.main(["install", "alpha", "--host", "all"]) == 2
    err = capsys.readouterr().err
    assert "--yes" in err
    assert not (claude / "alpha").exists()
    assert not (codex / "alpha").exists()

    assert cli.main(["install", "alpha", "--host", "all", "--yes"]) == 0
    assert (claude / "alpha" / "SKILL.md").exists()
    assert (codex / "alpha" / "SKILL.md").exists()


def test_install_creates_then_noop(tmp_path, monkeypatch, capsys):
    repo, target = _prep(tmp_path, monkeypatch)
    assert cli.main(["install"]) == 0
    assert (target / "alpha" / "SKILL.md").exists()
    capsys.readouterr()
    assert cli.main(["install"]) == 0
    assert "unchanged" in capsys.readouterr().out


def test_install_partial_failure_saves_successful_records(
    tmp_path,
    monkeypatch,
    capsys,
):
    repo, target = _make_two_skill_repo(tmp_path)
    state_root = tmp_path / "state"
    monkeypatch.chdir(repo)
    monkeypatch.setenv("XDG_STATE_HOME", str(state_root))
    real_copy_install = install_commands.copy_install

    def fail_beta(item, mode="copy"):
        if item.skill == "beta":
            raise OSError("disk full")
        return real_copy_install(item, mode=mode)

    monkeypatch.setattr(install_commands, "copy_install", fail_beta)

    assert cli.main(["install", "--host", "claude"]) == 1

    out = capsys.readouterr().out
    assert "created: alpha -> claude" in out
    assert "FAILED: beta -> claude (disk full)" in out
    assert (target / "alpha" / "SKILL.md").exists()
    assert not (target / "beta").exists()
    state = State.load(state_root / "my-skills" / "state.json")
    assert state.get("alpha", "claude") is not None
    assert state.get("beta", "claude") is None


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


def test_link_mode_installs_symlink_and_uninstall_keeps_source(tmp_path, monkeypatch, capsys):
    repo, target = _prep(tmp_path, monkeypatch)
    assert cli.main(["install", "--mode", "link"]) == 0
    dest = target / "alpha"
    assert dest.is_symlink()
    capsys.readouterr()
    # A linked install is always in sync with canonical.
    cli.main(["status"])
    assert "FRESH" in capsys.readouterr().out
    # Uninstall removes only the link; the canonical source survives.
    assert cli.main(["uninstall", "alpha"]) == 0
    assert not dest.exists() and not dest.is_symlink()
    assert (repo / "skills" / "alpha" / "SKILL.md").exists()


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


def test_uninstall_multi_host_requires_yes(tmp_path, monkeypatch, capsys):
    repo, claude, codex = _make_multi_host_repo(tmp_path)
    monkeypatch.chdir(repo)
    monkeypatch.setenv("XDG_STATE_HOME", str(tmp_path / "state"))

    assert cli.main(["install", "alpha", "--host", "all", "--yes"]) == 0
    capsys.readouterr()

    assert cli.main(["uninstall", "alpha", "--host", "all"]) == 2
    assert "--yes" in capsys.readouterr().err
    assert (claude / "alpha" / "SKILL.md").exists()
    assert (codex / "alpha" / "SKILL.md").exists()

    assert cli.main(["uninstall", "alpha", "--host", "all", "--yes"]) == 0
    assert not (claude / "alpha").exists()
    assert not (codex / "alpha").exists()


def test_uninstall_partial_failure_saves_successful_removals(
    tmp_path,
    monkeypatch,
    capsys,
):
    repo, claude, codex = _make_multi_host_repo(tmp_path)
    state_root = tmp_path / "state"
    monkeypatch.chdir(repo)
    monkeypatch.setenv("XDG_STATE_HOME", str(state_root))
    assert cli.main(["install", "alpha", "--host", "all", "--yes"]) == 0
    capsys.readouterr()
    real_uninstall = install_commands.uninstall

    def fail_codex(path):
        if path == codex / "alpha":
            raise OSError("permission denied")
        real_uninstall(path)

    monkeypatch.setattr(install_commands, "uninstall", fail_codex)

    assert cli.main(["uninstall", "alpha", "--host", "all", "--yes"]) == 1

    out = capsys.readouterr().out
    assert "removed: alpha -> claude" in out
    assert "FAILED: alpha -> codex (permission denied)" in out
    assert not (claude / "alpha").exists()
    assert (codex / "alpha" / "SKILL.md").exists()
    state = State.load(state_root / "my-skills" / "state.json")
    assert state.get("alpha", "claude") is None
    assert state.get("alpha", "codex") is not None
