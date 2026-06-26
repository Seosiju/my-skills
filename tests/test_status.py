from pathlib import Path

from my_skills.config import Defaults, Manifest, Skill, Target
from my_skills.hashing import hash_directory
from my_skills.planner import Action, Status, plan_install_one, status_of
from my_skills.state import InstallRecord, State


def _src(root: Path, content: str = "x") -> Path:
    d = root / "skills" / "alpha"
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


def _installed(root: Path, src: Path, content: str = "x") -> tuple[Path, State]:
    dest = root / "claude" / "alpha"
    dest.mkdir(parents=True)
    (dest / "SKILL.md").write_text(content)
    state = State(root / "state.json")
    state.put(InstallRecord(
        "alpha", "claude", "copy", str(src), str(dest),
        hash_directory(src), hash_directory(dest), "t",
    ))
    return dest, state


def _skill(m: Manifest) -> Skill:
    return m.skills["alpha"]


def test_status_missing(tmp_path):
    _src(tmp_path)
    m = _manifest(tmp_path)
    assert status_of(m, _skill(m), "claude", State(tmp_path / "s.json")) is Status.MISSING


def test_status_unmanaged(tmp_path):
    _src(tmp_path)
    m = _manifest(tmp_path)
    dest = m.targets["claude"].path / "alpha"
    dest.mkdir(parents=True)
    (dest / "SKILL.md").write_text("foreign")
    assert status_of(m, _skill(m), "claude", State(tmp_path / "s.json")) is Status.UNMANAGED


def test_status_fresh(tmp_path):
    src = _src(tmp_path)
    m = _manifest(tmp_path)
    _, state = _installed(tmp_path, src, "x")
    assert status_of(m, _skill(m), "claude", state) is Status.FRESH


def test_status_stale(tmp_path):
    src = _src(tmp_path)
    m = _manifest(tmp_path)
    _, state = _installed(tmp_path, src, "x")
    (src / "SKILL.md").write_text("changed-canonical")
    assert status_of(m, _skill(m), "claude", state) is Status.STALE


def test_status_drifted(tmp_path):
    src = _src(tmp_path)
    m = _manifest(tmp_path)
    dest, state = _installed(tmp_path, src, "x")
    (dest / "SKILL.md").write_text("changed-install")
    assert status_of(m, _skill(m), "claude", state) is Status.DRIFTED


def test_status_conflict(tmp_path):
    src = _src(tmp_path)
    m = _manifest(tmp_path)
    dest, state = _installed(tmp_path, src, "x")
    (src / "SKILL.md").write_text("changed-canonical")
    (dest / "SKILL.md").write_text("changed-install")
    assert status_of(m, _skill(m), "claude", state) is Status.CONFLICT


def test_status_unsupported(tmp_path):
    _src(tmp_path)
    m = _manifest(tmp_path)
    m.skills["alpha"].hosts = ["codex"]
    assert status_of(m, _skill(m), "claude", State(tmp_path / "s.json")) is Status.UNSUPPORTED


def test_plan_install_conflict_action(tmp_path):
    src = _src(tmp_path)
    m = _manifest(tmp_path)
    dest, state = _installed(tmp_path, src, "x")
    (src / "SKILL.md").write_text("changed-canonical")
    (dest / "SKILL.md").write_text("changed-install")
    item = plan_install_one(m, _skill(m), "claude", m.targets["claude"], state)
    assert item.action is Action.CONFLICT
