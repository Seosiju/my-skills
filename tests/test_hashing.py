from pathlib import Path

from my_skills.hashing import hash_directory


def _make(root: Path, files: dict[str, str]) -> Path:
    for rel, content in files.items():
        p = root / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(content)
    return root


def test_prefixed_and_deterministic(tmp_path):
    d = _make(tmp_path / "s", {"SKILL.md": "a", "scripts/x.py": "print(1)"})
    h1 = hash_directory(d)
    h2 = hash_directory(d)
    assert h1.startswith("sha256:")
    assert h1 == h2


def test_content_change_changes_hash(tmp_path):
    d1 = _make(tmp_path / "a", {"SKILL.md": "hello"})
    d2 = _make(tmp_path / "b", {"SKILL.md": "world"})
    assert hash_directory(d1) != hash_directory(d2)


def test_path_change_changes_hash(tmp_path):
    d1 = _make(tmp_path / "a", {"SKILL.md": "x"})
    d2 = _make(tmp_path / "b", {"OTHER.md": "x"})
    assert hash_directory(d1) != hash_directory(d2)


def test_location_independent(tmp_path):
    d1 = _make(tmp_path / "loc1" / "s", {"SKILL.md": "same", "r/y.txt": "z"})
    d2 = _make(tmp_path / "loc2" / "s", {"SKILL.md": "same", "r/y.txt": "z"})
    assert hash_directory(d1) == hash_directory(d2)


def test_runtime_artifacts_ignored(tmp_path):
    canonical = _make(tmp_path / "a", {"SKILL.md": "same"})
    installed = _make(
        tmp_path / "b",
        {
            "SKILL.md": "same",
            ".omc/state/idle-notif-cooldown.json": '{"lastSentAt": "x"}',
            "__pycache__/mod.pyc": "bytecode",
            ".DS_Store": "junk",
        },
    )
    # Runtime/system noise inside the install must not register as drift.
    assert hash_directory(canonical) == hash_directory(installed)
