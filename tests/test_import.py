from pathlib import Path

from my_skills import cli


def _repo(tmp_path: Path) -> Path:
    (tmp_path / "my-skills.toml").write_text(
        'schema_version = 1\nskills_root = "skills"\n\n'
        f'[targets.claude]\nenabled = true\nscope = "user"\npath = "{tmp_path / "h"}"\n'
    )
    (tmp_path / "skills").mkdir()
    return tmp_path


def _ext(tmp_path: Path, name: str = "brand", body: str = "# Brand\n") -> Path:
    d = tmp_path / "ext" / name
    d.mkdir(parents=True)
    (d / "SKILL.md").write_text(
        f"---\nname: {name}\ndescription: An imported skill. Use when importing.\n---\n\n{body}"
    )
    return d


def test_import_new_copies_in(tmp_path, monkeypatch, capsys):
    repo = _repo(tmp_path)
    ext = _ext(tmp_path)
    monkeypatch.chdir(repo)
    assert cli.main(["import", str(ext)]) == 0
    assert (repo / "skills" / "brand" / "SKILL.md").exists()


def test_import_identical_is_noop(tmp_path, monkeypatch, capsys):
    repo = _repo(tmp_path)
    ext = _ext(tmp_path)
    monkeypatch.chdir(repo)
    cli.main(["import", str(ext)])
    capsys.readouterr()
    assert cli.main(["import", str(ext)]) == 0
    assert "up to date" in capsys.readouterr().out


def test_import_differs_blocks_without_force(tmp_path, monkeypatch, capsys):
    repo = _repo(tmp_path)
    ext = _ext(tmp_path, body="# Brand new body\n")
    # Pre-existing canonical skill with different content.
    existing = repo / "skills" / "brand"
    existing.mkdir(parents=True)
    (existing / "SKILL.md").write_text(
        "---\nname: brand\ndescription: The original canonical version.\n---\n\n# Original\n"
    )
    monkeypatch.chdir(repo)
    assert cli.main(["import", str(ext)]) == 1
    assert "Original" in (existing / "SKILL.md").read_text()  # untouched


def test_import_force_overwrites(tmp_path, monkeypatch, capsys):
    repo = _repo(tmp_path)
    ext = _ext(tmp_path, body="# Brand new body\n")
    existing = repo / "skills" / "brand"
    existing.mkdir(parents=True)
    (existing / "SKILL.md").write_text(
        "---\nname: brand\ndescription: The original canonical version.\n---\n\n# Original\n"
    )
    monkeypatch.chdir(repo)
    assert cli.main(["import", str(ext), "--force"]) == 0
    assert "Brand new body" in (existing / "SKILL.md").read_text()


def test_import_no_skillmd_blocks(tmp_path, monkeypatch, capsys):
    repo = _repo(tmp_path)
    empty = tmp_path / "empty"
    empty.mkdir()
    monkeypatch.chdir(repo)
    assert cli.main(["import", str(empty)]) == 2


def test_import_invalid_source_blocks(tmp_path, monkeypatch, capsys):
    repo = _repo(tmp_path)
    bad = tmp_path / "ext" / "bad"
    bad.mkdir(parents=True)
    (bad / "SKILL.md").write_text("# no frontmatter here\n")
    monkeypatch.chdir(repo)
    assert cli.main(["import", str(bad)]) == 1
    assert not (repo / "skills" / "bad").exists()
