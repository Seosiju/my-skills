"""Local install state (plan section 11).

State records which canonical skill was installed to which host, in what mode,
with the source and installed content hashes. It is machine-local and never
committed. Writes are atomic (temp file + ``os.replace``).
"""

from __future__ import annotations

import json
import os
from dataclasses import MISSING, asdict, dataclass, fields
from pathlib import Path

SCHEMA_VERSION = 1


class StateError(ValueError):
    pass


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


_RECORD_FIELDS = {field.name for field in fields(InstallRecord)}
_REQUIRED_RECORD_FIELDS = {
    field.name
    for field in fields(InstallRecord)
    if field.default is MISSING and field.default_factory is MISSING
}


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
        schema_version = int(data.get("schema_version", 1))
        if schema_version > SCHEMA_VERSION:
            raise StateError(
                f"state file {resolved} was written by a newer my-skills; "
                f"upgrade the CLI, or move {resolved} aside and re-run install"
            )
        installs: dict[tuple[str, str], InstallRecord] = {}
        for index, raw in enumerate(data.get("installs", [])):
            if not isinstance(raw, dict):
                raise StateError(
                    f"state file {resolved} contains a malformed install record "
                    f"at index {index}; upgrade the CLI, or move {resolved} aside "
                    "and re-run install"
                )
            missing = sorted(_REQUIRED_RECORD_FIELDS - raw.keys())
            if missing:
                raise StateError(
                    f"state file {resolved} install record at index {index} is "
                    f"missing required field(s): {', '.join(missing)}; upgrade "
                    f"the CLI, or move {resolved} aside and re-run install"
                )
            record = InstallRecord(**{k: v for k, v in raw.items() if k in _RECORD_FIELDS})
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
