import os
from pathlib import Path

import pytest

import my_skills.installer as installer
from my_skills.hashing import hash_directory
from my_skills.installer import copy_install, uninstall
from my_skills.planner import Action, PlanItem


def _src(tmp: Path, content: str = "hello") -> Path:
    d = tmp / "src" / "alpha"
    (d / "scripts").mkdir(parents=True)
    (d / "SKILL.md").write_text(content)
    (d / "scripts" / "run.py").write_text("print('hi')")
    return d


def _item(src: Path, dest: Path, action: Action = Action.CREATE) -> PlanItem:
    return PlanItem("alpha", "claude", src, dest, action, source_hash=hash_directory(src))


def test_fresh_install(tmp_path):
    src = _src(tmp_path)
    dest = tmp_path / "hosts" / "claude" / "alpha"
    record = copy_install(_item(src, dest))
    assert (dest / "SKILL.md").read_text() == "hello"
    assert (dest / "scripts" / "run.py").exists()
    assert record.installed_hash == hash_directory(src)
    assert record.installed_at.endswith("Z")


def test_update_in_place(tmp_path):
    src = _src(tmp_path)
    dest = tmp_path / "hosts" / "claude" / "alpha"
    copy_install(_item(src, dest))
    (src / "SKILL.md").write_text("changed")
    record = copy_install(_item(src, dest, Action.UPDATE))
    assert (dest / "SKILL.md").read_text() == "changed"
    assert record.installed_hash == hash_directory(src)


def test_failure_restores_original(tmp_path, monkeypatch):
    src = _src(tmp_path)
    dest = tmp_path / "hosts" / "claude" / "alpha"
    copy_install(_item(src, dest))
    original_hash = hash_directory(dest)

    real_replace = os.replace
    calls = {"n": 0}

    def flaky_replace(a, b):
        calls["n"] += 1
        if calls["n"] == 2:  # the staged -> dest swap
            raise OSError("boom")
        return real_replace(a, b)

    monkeypatch.setattr(installer.os, "replace", flaky_replace)
    (src / "SKILL.md").write_text("newcontent")
    with pytest.raises(OSError):
        copy_install(_item(src, dest, Action.UPDATE))

    assert dest.exists()
    assert hash_directory(dest) == original_hash  # original preserved


def test_uninstall_removes(tmp_path):
    src = _src(tmp_path)
    dest = tmp_path / "hosts" / "claude" / "alpha"
    copy_install(_item(src, dest))
    uninstall(dest)
    assert not dest.exists()
