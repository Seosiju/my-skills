from __future__ import annotations

from pathlib import Path

from my_skills import cli
from my_skills.cli_runtime import _root_cache_path


def _make_registry(path: Path) -> Path:
    path.mkdir(parents=True)
    (path / "my-skills.toml").write_text(
        'schema_version = 1\nskills_root = "skills"\n',
        encoding="utf-8",
    )
    (path / "skills").mkdir()
    return path


def test_set_root_with_path_records_active_registry(tmp_path, capsys):
    registry = _make_registry(tmp_path / "registry")

    rc = cli.main(["set-root", str(registry)])

    out = capsys.readouterr().out
    assert rc == 0
    assert f"Active registry root set to {registry.resolve()}" in out
    assert _root_cache_path().read_text(encoding="utf-8").strip() == str(
        registry.resolve()
    )


def test_set_root_without_path_searches_cwd_only_not_cache(
    tmp_path, monkeypatch, capsys
):
    active = _make_registry(tmp_path / "active")
    elsewhere = tmp_path / "elsewhere"
    elsewhere.mkdir()
    assert cli.main(["set-root", str(active)]) == 0
    capsys.readouterr()
    monkeypatch.chdir(elsewhere)

    rc = cli.main(["set-root"])

    captured = capsys.readouterr()
    assert rc == 2
    assert "my-skills.toml not found" in captured.err
    assert _root_cache_path().read_text(encoding="utf-8").strip() == str(
        active.resolve()
    )


def test_set_root_rejects_non_registry_path(tmp_path, capsys):
    target = tmp_path / "not-registry"
    target.mkdir()

    rc = cli.main(["set-root", str(target)])

    captured = capsys.readouterr()
    assert rc == 2
    assert "does not contain my-skills.toml" in captured.err
