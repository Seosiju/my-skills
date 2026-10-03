import textwrap
from pathlib import Path

import pytest

from my_skills.config import (
    ManifestError,
    expand_path,
    load_manifest,
    selected_skills,
)

BASE = """
schema_version = 1
skills_root = "skills"

[targets.claude]
enabled = true
path = "~/.claude/skills"

[skills.alpha]
enabled = true
hosts = ["claude"]
source_type = "builtin-seed"
source_revision = "0.1.0"

[skills.beta]
enabled = false
hosts = ["claude"]
"""


def _write_manifest(root: Path, body: str = BASE) -> Path:
    (root / "my-skills.toml").write_text(textwrap.dedent(body))
    return root


def test_expand_path_is_absolute():
    p = expand_path("~/.claude/skills")
    assert p.is_absolute()
    assert str(p).endswith("/.claude/skills")


def test_load_manifest_reads_targets_and_skills(tmp_path):
    m = load_manifest(_write_manifest(tmp_path))
    assert m.schema_version == 1
    assert m.skills_dir == tmp_path / "skills"
    # Built-in targets are present even when not all are listed in the manifest.
    assert set(m.targets) >= {"claude", "codex", "hermes"}
    assert m.targets["claude"].path.is_absolute()
    assert set(m.skills) == {"alpha", "beta"}


def test_load_manifest_reads_skill_provenance(tmp_path):
    m = load_manifest(_write_manifest(tmp_path))

    assert m.skills["alpha"].source_type == "builtin-seed"
    assert m.skills["alpha"].source_revision == "0.1.0"
    assert m.skills["beta"].source_type == ""
    assert m.skills["beta"].source_revision == ""


def test_missing_manifest_raises(tmp_path):
    with pytest.raises(ManifestError, match="manifest not found"):
        load_manifest(tmp_path)


def test_selected_skills_enabled_default(tmp_path):
    m = load_manifest(_write_manifest(tmp_path))
    # Plan 5.5/9.5/9.6: bare selection includes only enabled=true skills.
    assert [s.name for s in selected_skills(m)] == ["alpha"]


def test_selected_skills_all_includes_disabled(tmp_path):
    m = load_manifest(_write_manifest(tmp_path))
    assert sorted(s.name for s in selected_skills(m, all=True)) == ["alpha", "beta"]


def test_selected_skills_explicit(tmp_path):
    m = load_manifest(_write_manifest(tmp_path))
    assert [s.name for s in selected_skills(m, explicit=["beta"])] == ["beta"]


def test_selected_skills_explicit_unknown_raises(tmp_path):
    m = load_manifest(_write_manifest(tmp_path))
    with pytest.raises(ManifestError, match="unknown skill"):
        selected_skills(m, explicit=["nope"])


def test_local_override_precedence(tmp_path):
    _write_manifest(tmp_path)
    (tmp_path / "my-skills.local.toml").write_text(
        '[targets.claude]\npath = "~/.claude/custom-skills"\n'
    )
    m = load_manifest(tmp_path)
    assert str(m.targets["claude"].path).endswith("/.claude/custom-skills")


def test_cli_override_beats_local(tmp_path):
    _write_manifest(tmp_path)
    (tmp_path / "my-skills.local.toml").write_text(
        '[targets.claude]\npath = "~/.claude/custom-skills"\n'
    )
    m = load_manifest(
        tmp_path, cli_overrides={"targets": {"claude": {"path": "/tmp/cli-skills"}}}
    )
    assert m.targets["claude"].path == Path("/tmp/cli-skills")


def test_agy_omitted_preserves_existing_default_selection(tmp_path):
    from my_skills.cli_runtime import resolve_hosts

    manifest = load_manifest(_write_manifest(tmp_path))
    assert resolve_hosts(manifest, "all") == ["claude", "codex", "hermes"]
    assert manifest.targets["agy"].enabled is False
    assert str(manifest.targets["agy"].path).endswith("/.gemini/config/skills")
    assert resolve_hosts(manifest, "agy") == ["agy"]


@pytest.mark.parametrize("enabled", [True, False])
def test_agy_explicit_activation_and_override_precedence(tmp_path, monkeypatch, enabled):
    monkeypatch.chdir(tmp_path)
    _write_manifest(tmp_path, BASE + f'\n[targets.agy]\nenabled = {str(enabled).lower()}\n')
    assert load_manifest(tmp_path).targets["agy"].enabled is enabled
    (tmp_path / "my-skills.local.toml").write_text(
        '[targets.agy]\nenabled = true\npath = "local-agy"\n'
    )
    local = load_manifest(tmp_path).targets["agy"]
    assert local.enabled is True
    assert local.path == tmp_path / "local-agy"
    override = load_manifest(tmp_path, cli_overrides={"targets": {"agy": {
        "enabled": False, "path": str(tmp_path / "cli-agy")
    }}}).targets["agy"]
    assert override.enabled is False
    assert override.path == tmp_path / "cli-agy"
