import subprocess
from pathlib import Path

from my_skills import bootstrap_commands, cli


def _make_repo(tmp_path: Path) -> tuple[Path, Path]:
    target = tmp_path / "hosts" / "codex"
    (tmp_path / "my-skills.toml").write_text(
        'schema_version = 1\nskills_root = "skills"\n\n'
        f'[targets.codex]\nenabled = true\nscope = "user"\npath = "{target}"\n\n'
        '[skills.my-skills]\nenabled = true\nhosts = ["codex"]\n'
    )
    skill = tmp_path / "skills" / "my-skills"
    skill.mkdir(parents=True)
    skill.joinpath("SKILL.md").write_text(
        "---\n"
        "name: my-skills\n"
        "description: A valid management skill for bootstrap tests.\n"
        "---\n\n"
        "# My Skills\n"
    )
    return tmp_path, target


def test_bootstrap_dry_run_prints_tool_install_without_writing_hosts(
    tmp_path, monkeypatch, capsys
):
    repo, target = _make_repo(tmp_path)
    monkeypatch.chdir(repo)
    monkeypatch.setenv("XDG_CONFIG_HOME", str(tmp_path / "config"))
    monkeypatch.setenv("XDG_STATE_HOME", str(tmp_path / "state"))

    rc = cli.main(["bootstrap", "--dry-run"])

    out = capsys.readouterr().out
    assert rc == 0
    assert "uv tool install --editable" in out
    assert str(repo) in out
    assert "Dry run" in out
    assert not target.exists()
    assert not (tmp_path / "state" / "my-skills" / "state.json").exists()
    assert not (tmp_path / "config" / "my-skills" / "root").exists()


def test_bootstrap_installs_cli_and_enabled_skills(tmp_path, monkeypatch, capsys):
    repo, target = _make_repo(tmp_path)
    monkeypatch.chdir(repo)
    monkeypatch.setenv("XDG_CONFIG_HOME", str(tmp_path / "config"))
    monkeypatch.setenv("XDG_STATE_HOME", str(tmp_path / "state"))

    calls: list[list[str]] = []

    def fake_run(command: list[str]) -> subprocess.CompletedProcess[str]:
        calls.append(command)
        return subprocess.CompletedProcess(command, 0)

    monkeypatch.setattr(bootstrap_commands.shutil, "which", lambda name: "/bin/uv")
    monkeypatch.setattr(bootstrap_commands.subprocess, "run", fake_run)

    rc = cli.main(["bootstrap"])

    out = capsys.readouterr().out
    assert rc == 0
    assert calls == [
        ["/bin/uv", "tool", "install", "--editable", str(repo), "--force", "--link-mode=copy"]
    ]
    assert not (tmp_path / "config" / "my-skills" / "root").exists()
    assert (target / "my-skills" / "SKILL.md").exists()
    assert "Cached repo root" not in out
    assert "Bootstrap complete. Run `my-skills doctor` to inspect registry configuration." in out


def test_bootstrap_reports_missing_uv_without_installing_skills(
    tmp_path, monkeypatch, capsys
):
    repo, target = _make_repo(tmp_path)
    monkeypatch.chdir(repo)
    monkeypatch.setenv("XDG_CONFIG_HOME", str(tmp_path / "config"))
    monkeypatch.setenv("XDG_STATE_HOME", str(tmp_path / "state"))
    monkeypatch.setattr(bootstrap_commands.shutil, "which", lambda name: None)

    rc = cli.main(["bootstrap"])

    err = capsys.readouterr().err
    assert rc == 2
    assert "uv command not found" in err
    assert not target.exists()
