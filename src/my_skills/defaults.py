from __future__ import annotations

from dataclasses import dataclass
from importlib import resources
from pathlib import Path
from typing import Final, TypeAlias


DefaultSeedSkill: TypeAlias = tuple[str, bool]

DEFAULT_SEED_SKILLS: Final[tuple[DefaultSeedSkill, ...]] = (
    ("cli-inventory", True),
    ("personal-profile", True),
    ("my-skills", True),
    ("my-jira", False),
)


@dataclass(frozen=True, slots=True)
class SeedUnavailable(Exception):
    source_seed_dir: Path

    def __str__(self) -> str:
        return f"default seed skills not found in package resources or {self.source_seed_dir}"


def package_repo_root() -> Path:
    return Path(__file__).resolve().parents[2]


def seed_skills_dir() -> Path:
    packaged = _packaged_seed_dir()
    if packaged is not None:
        return packaged

    source = package_repo_root() / "skills"
    if source.is_dir():
        return source

    raise SeedUnavailable(source_seed_dir=source)


def _packaged_seed_dir() -> Path | None:
    candidate = resources.files("my_skills").joinpath("_defaults", "skills")
    if not candidate.is_dir():
        return None
    if isinstance(candidate, Path):
        return candidate
    return None
