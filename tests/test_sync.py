import os
from pathlib import Path

import my_skills.installer as installer
from my_skills import cli


def _make_repo(tmp_path: Path) -> tuple[Path, Path, Path]:
    target = tmp_path / "hosts" / "claude"
    others = "\n".join(
        f'[targets.{h}]\nenabled = false\npath = "{tmp_path / "hosts" / h}"\n'
        for h in ("codex", "hermes")
    )
    (tmp_path / "my-skills.toml").write_text(
        'schema_version = 1\nskills_root = "skills"\n\n'
        f'[targets.claude]\nenabled = true\npath = "{target}"\n\n'
        f"{others}\n"
        '[skills.alpha]\nenabled = true\nhosts = ["claude"]\n'
    )
    skill = tmp_path / "skills" / "alpha"
    skill.mkdir(parents=True)
    skill.joinpath("SKILL.md").write_text(
        "---\nname: alpha\ndescription: A valid skill for sync tests.\n---\n\n# Alpha\n"
    )
    return tmp_path, target, skill


def _make_multi_host_repo(tmp_path: Path) -> tuple[Path, Path, Path, Path]:
    claude = tmp_path / "hosts" / "claude"
    codex = tmp_path / "hosts" / "codex"
    (tmp_path / "my-skills.toml").write_text(
        'schema_version = 1\nskills_root = "skills"\n\n'
        f'[targets.claude]\nenabled = true\npath = "{claude}"\n\n'
        f'[targets.codex]\nenabled = true\npath = "{codex}"\n\n'
        f'[targets.hermes]\nenabled = false\npath = "{tmp_path / "hosts" / "hermes"}"\n\n'
        '[skills.alpha]\nenabled = true\nhosts = ["claude", "codex"]\n'
    )
    skill = tmp_path / "skills" / "alpha"
    skill.mkdir(parents=True)
    skill.joinpath("SKILL.md").write_text(
        "---\nname: alpha\ndescription: A valid multi-host skill.\n---\n\n# Alpha\n"
    )
    return tmp_path, claude, codex, skill


def _prep(tmp_path, monkeypatch) -> tuple[Path, Path, Path]:
    repo, target, skill = _make_repo(tmp_path)
    monkeypatch.chdir(repo)
    monkeypatch.setenv("XDG_STATE_HOME", str(tmp_path / "state"))
    return repo, target, skill


def _edit_canonical(skill_dir: Path) -> None:
    p = skill_dir / "SKILL.md"
    p.write_text(p.read_text() + "\nedited canonical\n")


def test_sync_check_fresh_exit_zero(tmp_path, monkeypatch, capsys):
    repo, target, skill = _prep(tmp_path, monkeypatch)
    cli.main(["install"])
    capsys.readouterr()
    rc = cli.main(["sync", "--check"])
    assert rc == 0
    assert "FRESH" in capsys.readouterr().out


def test_sync_check_missing_nonzero(tmp_path, monkeypatch, capsys):
    _prep(tmp_path, monkeypatch)
    rc = cli.main(["sync", "--check"])
    assert rc == 1
    assert "MISSING" in capsys.readouterr().out


def test_sync_check_stale_nonzero_writes_nothing(tmp_path, monkeypatch, capsys):
    repo, target, skill = _prep(tmp_path, monkeypatch)
    cli.main(["install"])
    capsys.readouterr()
    _edit_canonical(skill)
    rc = cli.main(["sync", "--check"])
    out = capsys.readouterr().out
    assert rc == 1
    assert "STALE" in out
    assert "edited canonical" not in (target / "alpha" / "SKILL.md").read_text()


def test_sync_updates_stale_then_idempotent(tmp_path, monkeypatch, capsys):
    repo, target, skill = _prep(tmp_path, monkeypatch)
    cli.main(["install"])
    capsys.readouterr()
    _edit_canonical(skill)

    assert cli.main(["sync"]) == 0
    assert "updated" in capsys.readouterr().out
    assert "edited canonical" in (target / "alpha" / "SKILL.md").read_text()

    assert cli.main(["sync", "--check"]) == 0
    capsys.readouterr()
    cli.main(["sync"])
    assert "unchanged" in capsys.readouterr().out


def test_sync_multi_host_requires_yes(tmp_path, monkeypatch, capsys):
    repo, claude, codex, skill = _make_multi_host_repo(tmp_path)
    monkeypatch.chdir(repo)
    monkeypatch.setenv("XDG_STATE_HOME", str(tmp_path / "state"))

    assert cli.main(["install", "alpha", "--host", "all", "--yes"]) == 0
    capsys.readouterr()
    _edit_canonical(skill)

    assert cli.main(["sync", "alpha", "--host", "all", "--check"]) == 1
    assert "STALE" in capsys.readouterr().out

    assert cli.main(["sync", "alpha", "--host", "all"]) == 2
    assert "--yes" in capsys.readouterr().err
    assert "edited canonical" not in (claude / "alpha" / "SKILL.md").read_text()
    assert "edited canonical" not in (codex / "alpha" / "SKILL.md").read_text()

    assert cli.main(["sync", "alpha", "--host", "all", "--yes"]) == 0
    assert "edited canonical" in (claude / "alpha" / "SKILL.md").read_text()
    assert "edited canonical" in (codex / "alpha" / "SKILL.md").read_text()


def test_sync_missing_creates(tmp_path, monkeypatch, capsys):
    repo, target, skill = _prep(tmp_path, monkeypatch)
    assert cli.main(["sync"]) == 0
    assert (target / "alpha" / "SKILL.md").exists()


def test_sync_conflict_not_overwritten(tmp_path, monkeypatch, capsys):
    repo, target, skill = _prep(tmp_path, monkeypatch)
    cli.main(["install"])
    capsys.readouterr()
    _edit_canonical(skill)
    inst = target / "alpha" / "SKILL.md"
    inst.write_text(inst.read_text() + "\nlocal hand edit\n")

    rc = cli.main(["sync"])
    assert rc == 1
    assert "BLOCKED" in capsys.readouterr().out
    body = inst.read_text()
    assert "local hand edit" in body  # local change kept
    assert "edited canonical" not in body  # not overwritten with canonical


def test_sync_drifted_not_overwritten(tmp_path, monkeypatch, capsys):
    repo, target, skill = _prep(tmp_path, monkeypatch)
    cli.main(["install"])
    capsys.readouterr()
    inst = target / "alpha" / "SKILL.md"
    inst.write_text(inst.read_text() + "\nlocal hand edit\n")

    assert cli.main(["sync"]) == 1
    assert "local hand edit" in inst.read_text()


def test_sync_update_failure_preserves_install(tmp_path, monkeypatch, capsys):
    repo, target, skill = _prep(tmp_path, monkeypatch)
    cli.main(["install"])
    capsys.readouterr()
    inst = target / "alpha" / "SKILL.md"
    original = inst.read_text()
    _edit_canonical(skill)  # STALE -> sync will UPDATE

    real_replace = os.replace
    calls = {"n": 0}

    def flaky(a, b):
        calls["n"] += 1
        if calls["n"] == 2:
            raise OSError("boom")
        return real_replace(a, b)

    monkeypatch.setattr(installer.os, "replace", flaky)
    assert cli.main(["sync"]) == 1
    assert "FAILED: alpha -> claude (boom)" in capsys.readouterr().out

    assert inst.read_text() == original
    assert "edited canonical" not in inst.read_text()
