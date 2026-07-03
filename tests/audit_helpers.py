from __future__ import annotations

from pathlib import Path


def repo(tmp_path: Path, body: str = "# Alpha\n") -> tuple[Path, Path, Path]:
    target = tmp_path / "hosts" / "hermes"
    (tmp_path / "my-skills.toml").write_text(
        'schema_version = 1\nskills_root = "skills"\n\n'
        f'[targets.hermes]\nenabled = true\nscope = "user"\npath = "{target}"\n\n'
        '[skills.alpha]\nenabled = true\nhosts = ["hermes"]\n'
    )
    skill = tmp_path / "skills" / "alpha"
    skill.mkdir(parents=True)
    skill.joinpath("SKILL.md").write_text(
        "---\nname: alpha\ndescription: A valid skill for audit tests.\n---\n\n"
        f"{body}"
    )
    return tmp_path, target, skill


def external_skill(tmp_path: Path, body: str) -> Path:
    skill = tmp_path / "external" / "alpha"
    skill.mkdir(parents=True)
    skill.joinpath("SKILL.md").write_text(
        "---\nname: alpha\ndescription: A valid external skill.\n---\n\n"
        f"{body}"
    )
    return skill
