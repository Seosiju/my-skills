"""Local install state (plan section 11).

State records which canonical skill was installed to which host, in what mode,
with the source and installed content hashes. It is machine-local and never
committed. Writes are atomic (temp file + ``os.replace``).
"""

from __future__ import annotations

import json
import os
from dataclasses import asdict, dataclass
from pathlib import Path

SCHEMA_VERSION = 1


def default_state_path() -> Path:
    """Resolve the state file path, honoring ``XDG_STATE_HOME``."""
    xdg = os.environ.get("XDG_STATE_HOME")
    base = Path(xdg) if xdg else Path.home() / ".local" / "state"
    return base / "my-skills" / "state.json"


@dataclass
class InstallRecord:
    skill: str
    host: str
    mode: str
    source: str
    destination: str
    source_hash: str
    installed_hash: str
    installed_at: str
    source_type: str = "canonical"
    source_url: str = ""
    source_revision: str = ""
    last_audit_at: str = ""
    last_audit_result_hash: str = ""
    audit_profile: str = ""
    audit_threshold: str = ""

    @property
    def key(self) -> tuple[str, str]:
        return (self.skill, self.host)


class State:
    """In-memory view of the install state, keyed by ``(skill, host)``."""

    def __init__(self, path: Path, installs: dict[tuple[str, str], InstallRecord] | None = None):
        self.path = Path(path)
        self.installs: dict[tuple[str, str], InstallRecord] = installs or {}

    @classmethod
    def load(cls, path: Path | str | None = None) -> "State":
        resolved = Path(path) if path else default_state_path()
        if not resolved.is_file():
            return cls(resolved, {})
        data = json.loads(resolved.read_text(encoding="utf-8"))
        installs: dict[tuple[str, str], InstallRecord] = {}
        for raw in data.get("installs", []):
            record = InstallRecord(**raw)
            installs[record.key] = record
        return cls(resolved, installs)

    def get(self, skill: str, host: str) -> InstallRecord | None:
        return self.installs.get((skill, host))

    def put(self, record: InstallRecord) -> None:
        self.installs[record.key] = record

    def remove(self, skill: str, host: str) -> None:
        self.installs.pop((skill, host), None)

    def records(self) -> list[InstallRecord]:
        return [self.installs[k] for k in sorted(self.installs)]

    def save(self) -> None:
        self.path.parent.mkdir(parents=True, exist_ok=True)
        payload = {
            "schema_version": SCHEMA_VERSION,
            "installs": [asdict(r) for r in self.records()],
        }
        tmp = self.path.with_name(self.path.name + ".tmp")
        tmp.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
        os.replace(tmp, self.path)
