from __future__ import annotations

import pytest


@pytest.fixture(autouse=True)
def _isolate_user_dirs(tmp_path, monkeypatch):
    monkeypatch.setenv("XDG_CONFIG_HOME", str(tmp_path / "xdg-config"))
    monkeypatch.setenv("XDG_STATE_HOME", str(tmp_path / "xdg-state"))
    monkeypatch.setenv("XDG_DATA_HOME", str(tmp_path / "xdg-data"))
    monkeypatch.delenv("MY_SKILLS_ROOT", raising=False)
