from pathlib import Path

from my_skills.config import Defaults, Manifest, Skill, Target
from my_skills.hashing import hash_directory
from my_skills.planner import Action, plan_install_one, plan_uninstall_one
from my_skills.state import InstallRecord, State


def _skill_src(root: Path, name: str = "alpha", content: str = "x") -> Path:
    d = root / "skills" / name
    d.mkdir(parents=True)
    (d / "SKILL.md").write_text(content)
    return d


def _manifest(root: Path) -> Manifest:
    return Manifest(
        schema_version=1, skills_root="skills", defaults=Defaults(),
        targets={"claude": Target("claude", True, "user", root / "claude")},
        skills={"alpha": Skill("alpha", True, ["claude"])},
        root=root,
    )


def _managed(root, src, dest, *, source_hash=None, installed_hash=None) -> State:
    state = State(root / "state.json")
    state.put(InstallRecord(
        skill="alpha", host="claude", mode="copy",
        source=str(src), destination=str(dest),
        source_hash=source_hash or hash_directory(src),
        installed_hash=installed_hash or hash_directory(dest),
        installed_at="t",
    ))
    return state


def _install_one(m, state):
    return plan_install_one(m, m.skills["alpha"], "claude", m.targets["claude"], state)


def test_skip_unsupported(tmp_path):
    _skill_src(tmp_path)
    m = _manifest(tmp_path)
    m.skills["alpha"].hosts = ["codex"]
    assert _install_one(m, State(tmp_path / "s.json")).action is Action.SKIP_UNSUPPORTED


def test_create_when_missing(tmp_path):
    _skill_src(tmp_path)
    m = _manifest(tmp_path)
    assert _install_one(m, State(tmp_path / "s.json")).action is Action.CREATE


def test_block_conflict_unmanaged(tmp_path):
    _skill_src(tmp_path)
    m = _manifest(tmp_path)
    dest = m.targets["claude"].path / "alpha"
    dest.mkdir(parents=True)
    (dest / "SKILL.md").write_text("foreign")
    assert _install_one(m, State(tmp_path / "s.json")).action is Action.BLOCK_CONFLICT


def test_noop_when_managed_unchanged(tmp_path):
    src = _skill_src(tmp_path)
    m = _manifest(tmp_path)
    dest = m.targets["claude"].path / "alpha"
    dest.mkdir(parents=True)
    (dest / "SKILL.md").write_text("x")
    assert _install_one(m, _managed(tmp_path, src, dest)).action is Action.NOOP


def test_update_when_source_changed(tmp_path):
    src = _skill_src(tmp_path)
    m = _manifest(tmp_path)
    dest = m.targets["claude"].path / "alpha"
    dest.mkdir(parents=True)
    (dest / "SKILL.md").write_text("x")
    state = _managed(tmp_path, src, dest, source_hash="sha256:OLD")
    assert _install_one(m, state).action is Action.UPDATE


def test_block_drift_when_install_modified(tmp_path):
    src = _skill_src(tmp_path)
    m = _manifest(tmp_path)
    dest = m.targets["claude"].path / "alpha"
    dest.mkdir(parents=True)
    (dest / "SKILL.md").write_text("x")
    state = _managed(tmp_path, src, dest, installed_hash="sha256:DIFFERENT")
    assert _install_one(m, state).action is Action.BLOCK_DRIFT


def test_uninstall_not_managed(tmp_path):
    _skill_src(tmp_path)
    m = _manifest(tmp_path)
    item = plan_uninstall_one(m, "alpha", "claude", m.targets["claude"], State(tmp_path / "s.json"))
    assert item.action is Action.NOT_MANAGED


def test_uninstall_remove_managed(tmp_path):
    src = _skill_src(tmp_path)
    m = _manifest(tmp_path)
    dest = m.targets["claude"].path / "alpha"
    dest.mkdir(parents=True)
    (dest / "SKILL.md").write_text("x")
    item = plan_uninstall_one(m, "alpha", "claude", m.targets["claude"], _managed(tmp_path, src, dest))
    assert item.action is Action.REMOVE


def test_uninstall_block_drift(tmp_path):
    src = _skill_src(tmp_path)
    m = _manifest(tmp_path)
    dest = m.targets["claude"].path / "alpha"
    dest.mkdir(parents=True)
    (dest / "SKILL.md").write_text("x")
    state = _managed(tmp_path, src, dest, installed_hash="sha256:DIFF")
    item = plan_uninstall_one(m, "alpha", "claude", m.targets["claude"], state)
    assert item.action is Action.BLOCK_DRIFT
