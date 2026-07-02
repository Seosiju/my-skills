from pathlib import Path

import pytest

from my_skills.hosts.base import HostConfig
from my_skills.validation import (
    MAX_DESCRIPTION_LEN,
    validate_name,
    validate_skill,
    validate_skill_for_host,
)

FIXTURES = Path(__file__).parent / "fixtures"


def _errors(name: str) -> list[str]:
    return validate_skill(FIXTURES / name).errors


def test_good_skill_passes():
    result = validate_skill(FIXTURES / "good-skill")
    assert result.ok, result.errors
    assert result.errors == []


def test_missing_skill_md(tmp_path):
    d = tmp_path / "empty-skill"
    d.mkdir()
    result = validate_skill(d)
    assert not result.ok
    assert any("SKILL.md not found" in e for e in result.errors)


def test_no_frontmatter():
    assert any("missing opening" in e for e in _errors("no-frontmatter"))


def test_unterminated_frontmatter():
    assert any("unterminated" in e for e in _errors("unterminated-frontmatter"))


def test_malformed_frontmatter():
    assert any("malformed YAML" in e for e in _errors("malformed-frontmatter"))


def test_missing_name():
    assert any("missing required field 'name'" in e for e in _errors("missing-name"))


def test_missing_description():
    assert any(
        "missing required field 'description'" in e
        for e in _errors("missing-description")
    )


def test_name_mismatch():
    assert any("does not match frontmatter name" in e for e in _errors("name-mismatch"))


def test_bad_name_format():
    assert any("lowercase alphanumeric" in e for e in _errors("bad-name"))


def test_missing_reference():
    assert any(
        "referenced supporting file not found" in e
        for e in _errors("missing-reference")
    )


def test_abs_path_is_error_not_warning():
    result = validate_skill(FIXTURES / "abs-path-warning")
    assert not result.ok
    assert any("absolute host path" in e for e in result.errors)


def test_generic_host_default_paths_are_allowed(tmp_path: Path) -> None:
    skill = tmp_path / "host-paths"
    skill.mkdir()
    (skill / "SKILL.md").write_text(
        "---\n"
        "name: host-paths\n"
        "description: Documents default host paths.\n"
        "---\n"
        "\n"
        "# Host Paths\n"
        "\n"
        "Host directories (`~/.claude/skills`, `~/.agents/skills`, "
        "`~/.hermes/skills`) are build outputs.\n",
        encoding="utf-8",
    )

    result = validate_skill(skill)

    assert result.ok, result.errors


def test_long_description(tmp_path):
    d = tmp_path / "long-desc"
    d.mkdir()
    long_desc = "x" * (MAX_DESCRIPTION_LEN + 5)
    (d / "SKILL.md").write_text(
        f"---\nname: long-desc\ndescription: {long_desc}\n---\n\n# Body\n"
    )
    assert any("description exceeds" in e for e in validate_skill(d).errors)


def test_host_description_limit_is_enforced(tmp_path):
    d = tmp_path / "host-limited"
    d.mkdir()
    description = "x" * 90
    (d / "SKILL.md").write_text(
        f"---\nname: host-limited\ndescription: {description}\n---\n\n# Body\n"
    )
    host = HostConfig(
        name="tiny",
        display_name="Tiny Host",
        detect_commands=(),
        default_user_path="~/tiny",
        default_project_path=".tiny",
        supports_symlink=True,
        reload_hint="restart",
        description_max_chars=80,
    )

    result = validate_skill_for_host(d, host)

    assert not result.ok
    assert any("tiny description exceeds 80 characters" in e for e in result.errors)


@pytest.mark.parametrize(
    "name,valid",
    [
        ("good-skill", True),
        ("a", True),
        ("a1-b2-c3", True),
        ("Bad", False),
        ("-lead", False),
        ("trail-", False),
        ("double--hyphen", False),
        ("x" * 65, False),
    ],
)
def test_validate_name(name, valid):
    assert (validate_name(name) == []) is valid
