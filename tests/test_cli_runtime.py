from pathlib import Path

import pytest

from my_skills.cli_runtime import _root_cache_path, find_repo_root
from my_skills.config import ManifestError


def _make_root(path: Path) -> Path:
    (path / "my-skills.toml").write_text('schema_version = 1\n')
    return path


@pytest.fixture(autouse=True)
def _isolate_env(tmp_path, monkeypatch):
    """Redirect the cache under tmp and clear MY_SKILLS_ROOT for each test."""
    monkeypatch.delenv("MY_SKILLS_ROOT", raising=False)
    monkeypatch.setenv("XDG_CONFIG_HOME", str(tmp_path / "config"))


def test_env_override_wins(tmp_path, monkeypatch):
    root = tmp_path / "repo"
    root.mkdir(parents=True)
    _make_root(root)
    monkeypatch.setenv("MY_SKILLS_ROOT", str(root))
    assert find_repo_root(start=tmp_path) == root.resolve()


def test_env_override_invalid_raises(tmp_path, monkeypatch):
    monkeypatch.setenv("MY_SKILLS_ROOT", str(tmp_path / "nope"))
    with pytest.raises(ManifestError, match="MY_SKILLS_ROOT"):
        find_repo_root(start=tmp_path)


def test_cwd_search_finds_and_caches_first_run(tmp_path):
    root = tmp_path / "repo"
    (root / "sub").mkdir(parents=True)
    _make_root(root)
    assert find_repo_root(start=root / "sub") == root.resolve()
    # discovery writes the cache so later cwd-less runs resolve
    assert _root_cache_path().read_text().strip() == str(root.resolve())


def test_cwd_search_does_not_overwrite_different_valid_cache(
    tmp_path, capsys
):
    active = tmp_path / "active"
    cwd_root = tmp_path / "cwd-root"
    (cwd_root / "sub").mkdir(parents=True)
    active.mkdir()
    _make_root(active)
    _make_root(cwd_root)
    cache = _root_cache_path()
    cache.parent.mkdir(parents=True, exist_ok=True)
    cache.write_text(str(active.resolve()) + "\n")

    assert find_repo_root(start=cwd_root / "sub") == cwd_root.resolve()

    err = capsys.readouterr().err
    assert "active registry is" in err
    assert "my-skills set-root" in err
    assert cache.read_text(encoding="utf-8").strip() == str(active.resolve())


def test_cwd_search_replaces_invalid_cache_on_first_run(tmp_path):
    root = tmp_path / "repo"
    root.mkdir(parents=True)
    _make_root(root)
    cache = _root_cache_path()
    cache.parent.mkdir(parents=True, exist_ok=True)
    cache.write_text(str(tmp_path / "missing") + "\n")

    assert find_repo_root(start=root) == root.resolve()

    assert cache.read_text(encoding="utf-8").strip() == str(root.resolve())


def test_cache_fallback_when_not_under_root(tmp_path):
    root = tmp_path / "repo"
    root.mkdir(parents=True)
    _make_root(root)
    # seed the cache as a prior successful run would
    cache = _root_cache_path()
    cache.parent.mkdir(parents=True, exist_ok=True)
    cache.write_text(str(root.resolve()) + "\n")

    elsewhere = tmp_path / "elsewhere"
    elsewhere.mkdir()
    assert find_repo_root(start=elsewhere) == root.resolve()


def test_nothing_found_raises(tmp_path):
    elsewhere = tmp_path / "elsewhere"
    elsewhere.mkdir()
    with pytest.raises(ManifestError, match="set-root"):
        find_repo_root(start=elsewhere)
