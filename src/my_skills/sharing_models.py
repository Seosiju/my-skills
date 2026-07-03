from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True, slots=True)
class Risk:
    severity: str
    message: str


@dataclass(frozen=True, slots=True)
class ShareCandidate:
    name: str
    description: str
    source: Path
    canonical: Path
    canonical_status: str
    content_hash: str
    risks: tuple[Risk, ...]
    choices: tuple[str, ...]
    recommended: str


@dataclass(frozen=True, slots=True)
class SharePlan:
    from_host: str
    source: Path
    candidates: tuple[ShareCandidate, ...]


class ShareBlockedError(ValueError):
    pass


@dataclass(frozen=True, slots=True)
class ShareApplyResult:
    name: str
    source: Path
    canonical: Path
    enabled: bool
    hosts: tuple[str, ...]
    adopted_host: str
