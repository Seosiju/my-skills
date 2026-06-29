import json
from pathlib import Path

from my_skills import cli
from my_skills.config import load_manifest


def _skill(path: Path, name: str, description: str, body: str = "") -> Path:
    skill_dir = path / name
    skill_dir.mkdir(parents=True)
    skill_dir.joinpath("SKILL.md").write_text(
        f"---\nname: {name}\ndescription: {description}\n---\n\n# {name}\n{body}"
    )
    return skill_dir


def _repo_with_targets(tmp_path: Path) -> tuple[Path, Path, Path, Path]:
    repo = tmp_path / "repo"
    claude = tmp_path / "hosts" / "claude"
    codex = tmp_path / "hosts" / "codex"
    hermes = tmp_path / "hosts" / "hermes"
    repo.mkdir()
    (repo / "my-skills.toml").write_text(
        "schema_version = 1\n"
        'skills_root = "skills"\n\n'
        "[targets.claude]\n"
        "enabled = true\n"
        f'path = "{claude}"\n\n'
        "[targets.codex]\n"
        "enabled = true\n"
        f'path = "{codex}"\n\n'
        "[targets.hermes]\n"
        "enabled = true\n"
        f'path = "{hermes}"\n\n'
        "[skills.alpha]\n"
        "enabled = true\n"
        'hosts = ["claude", "codex", "hermes"]\n'
    )
    _skill(repo / "skills", "alpha", "Canonical alpha.")
    return repo, claude, codex, hermes


def test_install_dry_run_json_reports_plan_and_writes_nothing(
    tmp_path, monkeypatch, capsys
):
    repo, claude, _codex, _hermes = _repo_with_targets(tmp_path)
    monkeypatch.chdir(repo)
    monkeypatch.setenv("XDG_STATE_HOME", str(tmp_path / "state"))

    rc = cli.main(["install", "alpha", "--host", "claude", "--dry-run", "--json"])

    payload = json.loads(capsys.readouterr().out)
    assert rc == 0
    assert payload == {
        "actions": [
            {
                "skill": "alpha",
                "host": "claude",
                "action": "CREATE",
                "reason": "destination missing",
                "mode": "copy",
                "source": str(repo / "skills" / "alpha"),
                "destination": str(claude / "alpha"),
            }
        ]
    }
    assert not (claude / "alpha").exists()
    assert not (tmp_path / "state" / "my-skills" / "state.json").exists()


def test_share_plan_json_lists_host_candidates_and_risks(tmp_path, monkeypatch, capsys):
    repo, claude, _codex, _hermes = _repo_with_targets(tmp_path)
    _skill(claude, "brand", "A host-only skill.")
    bad = claude / "bad"
    bad.mkdir(parents=True)
    bad.joinpath("SKILL.md").write_text("# missing frontmatter\n")
    monkeypatch.chdir(repo)

    rc = cli.main(["share", "--from", "claude", "--plan", "--json"])

    payload = json.loads(capsys.readouterr().out)
    assert rc == 0
    assert payload["from"] == "claude"
    assert payload["source"] == str(claude)
    assert [candidate["name"] for candidate in payload["candidates"]] == ["bad", "brand"]

    bad_plan = payload["candidates"][0]
    assert bad_plan["canonical_status"] == "missing"
    assert bad_plan["recommended"] == "skip"
    assert bad_plan["choices"] == ["skip"]
    assert bad_plan["risks"][0]["severity"] == "error"
    assert "frontmatter" in bad_plan["risks"][0]["message"]

    brand_plan = payload["candidates"][1]
    assert brand_plan["description"] == "A host-only skill."
    assert brand_plan["canonical_status"] == "missing"
    assert brand_plan["recommended"] == "share-enable"
    assert brand_plan["choices"] == ["share-enable", "share-disable", "skip"]
    assert brand_plan["risks"] == []


def test_share_apply_imports_registers_adopts_source_and_syncs(
    tmp_path, monkeypatch, capsys
):
    repo, claude, codex, _hermes = _repo_with_targets(tmp_path)
    _skill(claude, "repo-analysis", "A host-only skill.")
    monkeypatch.chdir(repo)
    monkeypatch.setenv("XDG_STATE_HOME", str(tmp_path / "state"))

    assert cli.main(["share", "--from", "claude", "repo-analysis", "--enable"]) == 0

    manifest = load_manifest(repo)
    assert manifest.skills["repo-analysis"].enabled is True
    assert manifest.skills["repo-analysis"].hosts == ["claude", "codex", "hermes"]
    assert (repo / "skills" / "repo-analysis" / "SKILL.md").exists()

    assert cli.main(["sync", "repo-analysis"]) == 0
    assert "unchanged: repo-analysis -> claude" in capsys.readouterr().out
    assert (codex / "repo-analysis" / "SKILL.md").exists()


def test_share_apply_blocks_different_canonical_without_force(
    tmp_path, monkeypatch, capsys
):
    repo, claude, _codex, _hermes = _repo_with_targets(tmp_path)
    _skill(claude, "brand", "A host-only skill.", body="host body\n")
    _skill(repo / "skills", "brand", "Existing canonical.", body="canonical body\n")
    monkeypatch.chdir(repo)

    assert cli.main(["share", "--from", "claude", "brand", "--enable"]) == 1
    assert "canonical body" in (repo / "skills" / "brand" / "SKILL.md").read_text()

    assert cli.main(["share", "--from", "claude", "brand", "--enable", "--force"]) == 0
    assert "host body" in (repo / "skills" / "brand" / "SKILL.md").read_text()


def test_share_apply_disable_registers_disabled_skill(tmp_path, monkeypatch, capsys):
    repo, claude, _codex, _hermes = _repo_with_targets(tmp_path)
    _skill(claude, "brand", "A host-only skill.")
    monkeypatch.chdir(repo)

    assert cli.main(["share", "--from", "claude", "brand", "--disable"]) == 0

    manifest = load_manifest(repo)
    assert manifest.skills["brand"].enabled is False
    assert (repo / "skills" / "brand" / "SKILL.md").exists()
