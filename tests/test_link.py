from pathlib import Path

from my_skills.config import Defaults, Manifest, Skill, Target
from my_skills.installer import link_install, uninstall
from my_skills.planner import (
    Action,
    PlanItem,
    Status,
    is_managed_link,
    plan_install_one,
    plan_uninstall_one,
    status_of,
)
from my_skills.state import State


def _setup(root: Path) -> tuple[Path, Manifest]:
    src = root / "skills" / "alpha"
    src.mkdir(parents=True)
    (src / "SKILL.md").write_text(
        "---\nname: alpha\ndescription: A link-mode test skill.\n---\n\n# Alpha\n"
    )
    m = Manifest(
        schema_version=1, skills_root="skills", defaults=Defaults(),
        targets={"claude": Target("claude", True, "user", root / "claude")},
        skills={"alpha": Skill("alpha", True, ["claude"])},
        root=root,
    )
    return src, m


def _link(root: Path, src: Path, m: Manifest) -> tuple[Path, State]:
    dest = m.targets["claude"].path / "alpha"
    rec = link_install(PlanItem("alpha", "claude", src, dest, Action.CREATE, mode="link"))
    state = State(root / "s.json")
    state.put(rec)
    return dest, state


def test_link_install_creates_symlink(tmp_path):
    src, m = _setup(tmp_path)
    dest, _ = _link(tmp_path, src, m)
    assert dest.is_symlink()
    assert is_managed_link(dest, src)
    assert (dest / "SKILL.md").read_text() == (src / "SKILL.md").read_text()


def test_link_record_mode_is_link(tmp_path):
    src, m = _setup(tmp_path)
    _, state = _link(tmp_path, src, m)
    assert state.get("alpha", "claude").mode == "link"


def test_link_status_fresh(tmp_path):
    src, m = _setup(tmp_path)
    _, state = _link(tmp_path, src, m)
    assert status_of(m, m.skills["alpha"], "claude", state) is Status.FRESH


def test_link_status_fresh_even_after_canonical_edit(tmp_path):
    # A link always reflects canonical, so editing the source keeps it FRESH.
    src, m = _setup(tmp_path)
    _, state = _link(tmp_path, src, m)
    (src / "SKILL.md").write_text("---\nname: alpha\ndescription: edited.\n---\n\n# Edited\n")
    assert status_of(m, m.skills["alpha"], "claude", state) is Status.FRESH


def test_link_status_missing_when_link_removed(tmp_path):
    src, m = _setup(tmp_path)
    dest, state = _link(tmp_path, src, m)
    dest.unlink()
    assert status_of(m, m.skills["alpha"], "claude", state) is Status.MISSING


def test_link_status_drifted_when_replaced_by_dir(tmp_path):
    src, m = _setup(tmp_path)
    dest, state = _link(tmp_path, src, m)
    dest.unlink()
    dest.mkdir()
    assert status_of(m, m.skills["alpha"], "claude", state) is Status.DRIFTED


def test_link_plan_noop_when_intact(tmp_path):
    src, m = _setup(tmp_path)
    _, state = _link(tmp_path, src, m)
    item = plan_install_one(m, m.skills["alpha"], "claude", m.targets["claude"], state)
    assert item.action is Action.NOOP
    assert item.mode == "link"


def test_link_plan_recreates_when_removed(tmp_path):
    src, m = _setup(tmp_path)
    dest, state = _link(tmp_path, src, m)
    dest.unlink()
    item = plan_install_one(m, m.skills["alpha"], "claude", m.targets["claude"], state)
    # Re-link (not copy): a missing link record plans CREATE in link mode.
    assert item.action is Action.CREATE
    assert item.mode == "link"


def test_link_uninstall_keeps_canonical_source(tmp_path):
    src, m = _setup(tmp_path)
    dest, state = _link(tmp_path, src, m)
    item = plan_uninstall_one(m, "alpha", "claude", m.targets["claude"], state)
    assert item.action is Action.REMOVE
    uninstall(item.destination)
    assert not dest.exists() and not dest.is_symlink()
    # The canonical source must survive — the symlink target is never deleted.
    assert (src / "SKILL.md").is_file()


def test_link_uninstall_blocks_when_replaced(tmp_path):
    src, m = _setup(tmp_path)
    dest, state = _link(tmp_path, src, m)
    dest.unlink()
    dest.mkdir()  # replaced by a real directory
    item = plan_uninstall_one(m, "alpha", "claude", m.targets["claude"], state)
    assert item.action is Action.BLOCK_DRIFT
