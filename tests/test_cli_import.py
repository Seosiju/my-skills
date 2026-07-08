from __future__ import annotations

import shutil
from pathlib import Path

from my_skills import cli
from my_skills.config import load_manifest


def _repo(tmp_path: Path) -> Path:
    claude_path = tmp_path / "hosts" / "claude"
    codex_path = tmp_path / "hosts" / "codex"
    hermes_path = tmp_path / "hosts" / "hermes"
    (tmp_path / "my-skills.toml").write_text(
        'schema_version = 1\nskills_root = "skills"\n\n'
        f'[targets.claude]\nenabled = true\nscope = "user"\npath = "{claude_path}"\n\n'
        f'[targets.codex]\nenabled = true\nscope = "user"\npath = "{codex_path}"\n\n'
        f'[targets.hermes]\nenabled = false\nscope = "user"\npath = "{hermes_path}"\n',
        encoding="utf-8",
    )
    (tmp_path / "skills").mkdir()
    return tmp_path


def _skill(tmp_path: Path, name: str = "brand", body: str = "# Brand\n") -> Path:
    skill = tmp_path / "external" / name
    skill.mkdir(parents=True)
    skill.joinpath("SKILL.md").write_text(
        f"---\nname: {name}\ndescription: A focused import test skill.\n---\n\n{body}",
        encoding="utf-8",
    )
    return skill


def test_import_registers_new_skill_disabled_with_enabled_targets(
    tmp_path: Path,
    monkeypatch,
    capsys,
) -> None:
    repo = _repo(tmp_path)
    skill = _skill(tmp_path)
    monkeypatch.chdir(repo)

    assert cli.main(["import", str(skill)]) == 0

    manifest = load_manifest(repo)
    assert manifest.skills["brand"].enabled is False
    assert manifest.skills["brand"].hosts == ["claude", "codex"]
    text = (repo / "my-skills.toml").read_text(encoding="utf-8")
    assert "[skills.brand]" in text
    assert "source_type" not in text
    assert "source_revision" not in text
    assert "add [skills.brand]" not in capsys.readouterr().out


def test_import_enable_registers_new_skill_enabled(
    tmp_path: Path,
    monkeypatch,
) -> None:
    repo = _repo(tmp_path)
    skill = _skill(tmp_path)
    monkeypatch.chdir(repo)

    assert cli.main(["import", "--enable", str(skill)]) == 0

    assert load_manifest(repo).skills["brand"].enabled is True


def test_imported_skill_works_with_enable_skills_and_install_surfaces(
    tmp_path: Path,
    monkeypatch,
    capsys,
) -> None:
    repo = _repo(tmp_path)
    skill = _skill(tmp_path)
    monkeypatch.chdir(repo)

    assert cli.main(["import", str(skill)]) == 0
    capsys.readouterr()
    assert cli.main(["enable", "brand"]) == 0
    assert cli.main(["skills"]) == 0
    assert "brand" in capsys.readouterr().out
    assert cli.main(["install", "brand", "--host", "claude", "--dry-run"]) == 0
    assert "CREATE" in capsys.readouterr().out


def test_import_registers_when_canonical_directory_is_identical_but_unregistered(
    tmp_path: Path,
    monkeypatch,
    capsys,
) -> None:
    repo = _repo(tmp_path)
    skill = _skill(tmp_path)
    shutil.copytree(skill, repo / "skills" / "brand")
    monkeypatch.chdir(repo)

    assert cli.main(["import", str(skill)]) == 0

    assert load_manifest(repo).skills["brand"].enabled is False
    out = capsys.readouterr().out
    assert "up to date" in out
    assert "registered: brand (disabled)" in out


def test_import_preserves_existing_registered_section_without_enable(
    tmp_path: Path,
    monkeypatch,
) -> None:
    repo = _repo(tmp_path)
    skill = _skill(tmp_path)
    shutil.copytree(skill, repo / "skills" / "brand")
    with (repo / "my-skills.toml").open("a", encoding="utf-8") as file:
        file.write('\n[skills.brand]\nenabled = true\nhosts = ["claude"]\n')
    monkeypatch.chdir(repo)

    assert cli.main(["import", str(skill)]) == 0

    skill_config = load_manifest(repo).skills["brand"]
    assert skill_config.enabled is True
    assert skill_config.hosts == ["claude"]


def test_import_enable_existing_disabled_skill_preserves_hosts(
    tmp_path: Path,
    monkeypatch,
) -> None:
    repo = _repo(tmp_path)
    skill = _skill(tmp_path)
    shutil.copytree(skill, repo / "skills" / "brand")
    with (repo / "my-skills.toml").open("a", encoding="utf-8") as file:
        file.write('\n[skills.brand]\nenabled = false\nhosts = ["claude"]\n')
    monkeypatch.chdir(repo)

    assert cli.main(["import", "--enable", str(skill)]) == 0

    skill_config = load_manifest(repo).skills["brand"]
    assert skill_config.enabled is True
    assert skill_config.hosts == ["claude"]


def test_import_existing_registered_skill_does_not_bake_local_overlay_into_main_manifest(
    tmp_path: Path,
    monkeypatch,
) -> None:
    repo = _repo(tmp_path)
    skill = _skill(tmp_path)
    shutil.copytree(skill, repo / "skills" / "brand")
    with (repo / "my-skills.toml").open("a", encoding="utf-8") as file:
        file.write('\n[skills.brand]\nenabled = false\nhosts = ["claude"]\n')
    (repo / "my-skills.local.toml").write_text(
        '[skills.brand]\nenabled = true\nhosts = ["codex"]\n',
        encoding="utf-8",
    )
    original = (repo / "my-skills.toml").read_text(encoding="utf-8")
    monkeypatch.chdir(repo)

    assert cli.main(["import", str(skill)]) == 0

    assert (repo / "my-skills.toml").read_text(encoding="utf-8") == original


def test_import_enable_existing_skill_updates_main_manifest_despite_local_overlay(
    tmp_path: Path,
    monkeypatch,
) -> None:
    repo = _repo(tmp_path)
    skill = _skill(tmp_path)
    shutil.copytree(skill, repo / "skills" / "brand")
    with (repo / "my-skills.toml").open("a", encoding="utf-8") as file:
        file.write('\n[skills.brand]\nenabled = false\nhosts = ["claude"]\n')
    (repo / "my-skills.local.toml").write_text(
        '[skills.brand]\nenabled = true\nhosts = ["codex"]\n',
        encoding="utf-8",
    )
    monkeypatch.chdir(repo)

    assert cli.main(["import", "--enable", str(skill)]) == 0

    main_manifest = (repo / "my-skills.toml").read_text(encoding="utf-8")
    assert '[skills.brand]\nenabled = true\nhosts = ["claude"]' in main_manifest


def test_import_registers_skill_present_only_in_local_overlay(
    tmp_path: Path,
    monkeypatch,
) -> None:
    repo = _repo(tmp_path)
    skill = _skill(tmp_path)
    shutil.copytree(skill, repo / "skills" / "brand")
    (repo / "my-skills.local.toml").write_text(
        '[skills.brand]\nenabled = true\nhosts = ["codex"]\n',
        encoding="utf-8",
    )
    monkeypatch.chdir(repo)

    assert cli.main(["import", str(skill)]) == 0

    main_manifest = (repo / "my-skills.toml").read_text(encoding="utf-8")
    expected = '[skills.brand]\nenabled = false\nhosts = ["claude", "codex"]'
    assert expected in main_manifest


def test_import_enable_registers_skill_present_only_in_local_overlay(
    tmp_path: Path,
    monkeypatch,
) -> None:
    repo = _repo(tmp_path)
    skill = _skill(tmp_path)
    shutil.copytree(skill, repo / "skills" / "brand")
    (repo / "my-skills.local.toml").write_text(
        '[skills.brand]\nenabled = false\nhosts = ["codex"]\n',
        encoding="utf-8",
    )
    monkeypatch.chdir(repo)

    assert cli.main(["import", "--enable", str(skill)]) == 0

    main_manifest = (repo / "my-skills.toml").read_text(encoding="utf-8")
    expected = '[skills.brand]\nenabled = true\nhosts = ["claude", "codex"]'
    assert expected in main_manifest


def test_import_validation_block_does_not_modify_manifest(
    tmp_path: Path,
    monkeypatch,
) -> None:
    repo = _repo(tmp_path)
    skill = _skill(tmp_path)
    skill.joinpath("SKILL.md").write_text(
        "---\nname: brand\n---\n\n# Missing description\n",
        encoding="utf-8",
    )
    original = (repo / "my-skills.toml").read_text(encoding="utf-8")
    monkeypatch.chdir(repo)

    assert cli.main(["import", str(skill)]) == 1

    assert (repo / "my-skills.toml").read_text(encoding="utf-8") == original
    assert not (repo / "skills" / "brand").exists()
