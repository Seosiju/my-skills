from __future__ import annotations

import json
import subprocess
import sys
import tomllib
from pathlib import Path

import pytest

from my_skills import __version__
from my_skills import cli
import my_skills.init_registry_commands as init_registry_commands
from my_skills.defaults import DEFAULT_SEED_SKILLS


SEED_HOSTS = ["claude", "codex", "hermes"]


def _init_registry(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> Path:
    home = tmp_path / "home"
    home.mkdir()
    monkeypatch.setenv("HOME", str(home))
    monkeypatch.setenv("XDG_STATE_HOME", str(home / ".local" / "state"))

    target = tmp_path / "my-agent-skills"
    assert cli.main(["init-registry", str(target)]) == 0
    return target


def test_init_registry_registers_seed_skills_in_manifest(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    target = _init_registry(tmp_path, monkeypatch)
    capsys.readouterr()

    manifest = tomllib.loads((target / "my-skills.toml").read_text(encoding="utf-8"))

    assert set(manifest["skills"]) == {name for name, _enabled in DEFAULT_SEED_SKILLS}
    for name, enabled in DEFAULT_SEED_SKILLS:
        assert manifest["skills"][name]["enabled"] is enabled
        assert manifest["skills"][name]["hosts"] == SEED_HOSTS
        assert manifest["skills"][name]["source_type"] == "builtin-seed"
        assert manifest["skills"][name]["source_revision"] == __version__


def test_init_registry_install_dry_run_has_seed_actions(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    target = _init_registry(tmp_path, monkeypatch)
    monkeypatch.chdir(target)
    capsys.readouterr()

    rc = cli.main(["install", "--dry-run", "--json"])

    payload = json.loads(capsys.readouterr().out)
    assert rc == 0
    assert payload["actions"]
    action_skills = {action["skill"] for action in payload["actions"]}
    assert {"cli-inventory", "personal-profile", "my-skills"} <= action_skills
    assert "my-jira" not in action_skills


def test_seed_default_install_requires_confirmation_for_custom_targets(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    target = _init_registry(tmp_path, monkeypatch)
    (target / "my-skills.local.toml").write_text(
        '[targets.claude]\npath = "custom/claude"\n',
        encoding="utf-8",
    )
    monkeypatch.chdir(target)
    capsys.readouterr()

    rc = cli.main(["install"])

    captured = capsys.readouterr()
    assert rc == 2
    assert "writes to multiple hosts" in captured.err


def test_init_registry_no_defaults_has_no_skill_manifest_entries(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    home = tmp_path / "home"
    home.mkdir()
    monkeypatch.setenv("HOME", str(home))

    target = tmp_path / "my-agent-skills"
    assert cli.main(["init-registry", str(target), "--no-defaults"]) == 0
    capsys.readouterr()

    manifest = tomllib.loads((target / "my-skills.toml").read_text(encoding="utf-8"))
    assert "skills" not in manifest


def test_init_registry_caches_root_for_later_commands(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    home = tmp_path / "home"
    xdg = tmp_path / "xdg"
    elsewhere = tmp_path / "elsewhere"
    home.mkdir()
    elsewhere.mkdir()
    monkeypatch.setenv("HOME", str(home))
    monkeypatch.setenv("XDG_CONFIG_HOME", str(xdg))

    target = tmp_path / "my-agent-skills"
    assert cli.main(["init-registry", str(target)]) == 0
    capsys.readouterr()
    monkeypatch.chdir(elsewhere)

    rc = cli.main(["skills", "--json"])

    payload = json.loads(capsys.readouterr().out)
    assert rc == 0
    assert {row["name"] for row in payload["skills"]} == {
        name for name, _enabled in DEFAULT_SEED_SKILLS
    }


def test_init_registry_without_path_uses_default_location_when_not_tty(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    home = tmp_path / "home"
    home.mkdir()
    monkeypatch.setenv("HOME", str(home))

    assert cli.main(["init-registry", "--no-defaults"]) == 0

    out = capsys.readouterr().out
    assert f"Created private skill registry at {home / 'my-agent-skills'}" in out
    assert (home / "my-agent-skills" / "my-skills.toml").is_file()


def test_init_registry_prompt_accepts_empty_default(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    home = tmp_path / "home"
    home.mkdir()
    monkeypatch.setenv("HOME", str(home))
    monkeypatch.setattr(sys.stdin, "isatty", lambda: True)
    monkeypatch.setattr("builtins.input", lambda _prompt: "")

    assert cli.main(["init-registry", "--no-defaults"]) == 0

    capsys.readouterr()
    assert (home / "my-agent-skills" / "my-skills.toml").is_file()


def test_init_registry_prompt_reasks_for_existing_registry(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    existing = tmp_path / "existing"
    alternate = tmp_path / "alternate"
    existing.mkdir()
    (existing / "my-skills.toml").write_text("schema_version = 1\n", encoding="utf-8")
    answers = iter((str(existing), str(alternate)))
    monkeypatch.setattr(sys.stdin, "isatty", lambda: True)
    monkeypatch.setattr("builtins.input", lambda _prompt: next(answers))

    assert cli.main(["init-registry", "--no-defaults"]) == 0

    out = capsys.readouterr().out
    assert "already contains a my-skills registry" in out
    assert (alternate / "my-skills.toml").is_file()
    assert not (tmp_path / "alternate (1)").exists()


def test_init_registry_initializes_git_repo_by_default(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    target = tmp_path / "my-agent-skills"

    assert cli.main(["init-registry", str(target), "--no-defaults"]) == 0

    capsys.readouterr()
    assert (target / ".git").is_dir()


def test_init_registry_initial_commit_does_not_track_preexisting_local_state(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    target = tmp_path / "my-agent-skills"
    (target / ".omc" / "state").mkdir(parents=True)
    (target / ".omc" / "state" / "private.txt").write_text("secret\n", encoding="utf-8")

    assert cli.main(["init-registry", str(target), "--no-defaults"]) == 0

    capsys.readouterr()
    tracked = subprocess.run(
        ["git", "ls-files"],
        cwd=target,
        text=True,
        capture_output=True,
        check=True,
    ).stdout.splitlines()
    assert ".omc/state/private.txt" not in tracked
    assert {".gitignore", "README.md", "my-skills.toml"} <= set(tracked)


def test_init_registry_seed_copy_does_not_copy_private_source_files(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
) -> None:
    source = tmp_path / "source-skills"
    for name, _enabled in DEFAULT_SEED_SKILLS:
        skill = source / name
        skill.mkdir(parents=True)
        skill.joinpath("SKILL.md").write_text(
            f"---\nname: {name}\ndescription: Test seed.\n---\n\n# {name}\n",
            encoding="utf-8",
        )
    (source / "cli-inventory" / "references").mkdir()
    (source / "cli-inventory" / "references" / "inventory-schema.md").write_text(
        "# Schema\n",
        encoding="utf-8",
    )
    (source / "cli-inventory" / "scripts").mkdir()
    (source / "cli-inventory" / "scripts" / "scan_tools.py").write_text(
        "print('scan')\n",
        encoding="utf-8",
    )
    (source / "personal-profile" / "references").mkdir()
    (source / "personal-profile" / "references" / "schema.md").write_text(
        "# Schema\n",
        encoding="utf-8",
    )
    (source / "my-jira" / "config.example.json").write_text(
        "{}\n",
        encoding="utf-8",
    )
    (source / "my-jira" / "config.json").write_text(
        '{"token": "private"}\n',
        encoding="utf-8",
    )
    monkeypatch.setattr(init_registry_commands, "seed_skills_dir", lambda: source)
    target = tmp_path / "my-agent-skills"

    assert cli.main(["init-registry", str(target)]) == 0

    capsys.readouterr()
    assert not (target / "skills" / "my-jira" / "config.json").exists()
    tracked = subprocess.run(
        ["git", "ls-files"],
        cwd=target,
        text=True,
        capture_output=True,
        check=True,
    ).stdout.splitlines()
    assert "skills/my-jira/config.json" not in tracked


def test_init_registry_no_git_skips_git_init(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    target = tmp_path / "my-agent-skills"

    assert cli.main(["init-registry", str(target), "--no-defaults", "--no-git"]) == 0

    capsys.readouterr()
    assert not (target / ".git").exists()


def test_init_registry_missing_git_is_graceful(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    target = tmp_path / "my-agent-skills"
    monkeypatch.setattr(init_registry_commands.shutil, "which", lambda _name: None)

    assert cli.main(["init-registry", str(target), "--no-defaults"]) == 0

    out = capsys.readouterr().out
    assert "git command not found; skipped git init" in out
    assert not (target / ".git").exists()
