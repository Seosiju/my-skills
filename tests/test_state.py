import pytest

from my_skills.state import InstallRecord, State, StateError, default_state_path


def _rec(skill="s", host="claude"):
    return InstallRecord(
        skill=skill, host=host, mode="copy",
        source="/src/" + skill, destination="/dst/" + skill,
        source_hash="sha256:aa", installed_hash="sha256:bb",
        installed_at="2026-06-26T00:00:00Z",
    )


def test_load_absent_is_empty(tmp_path):
    assert State.load(tmp_path / "nope.json").installs == {}


def test_save_reload_roundtrip(tmp_path):
    path = tmp_path / "state.json"
    state = State(path)
    state.put(_rec("alpha"))
    state.put(_rec("beta", "codex"))
    state.save()

    again = State.load(path)
    assert set(again.installs) == {("alpha", "claude"), ("beta", "codex")}
    alpha = again.get("alpha", "claude")
    assert alpha is not None
    assert alpha.mode == "copy"


def test_load_ignores_unknown_record_fields(tmp_path):
    path = tmp_path / "state.json"
    state = State(path)
    state.put(_rec("alpha"))
    state.save()
    text = path.read_text(encoding="utf-8")
    path.write_text(
        text.replace('"installed_at":', '"future_field": "ignored",\n      "installed_at":'),
        encoding="utf-8",
    )

    again = State.load(path)

    alpha = again.get("alpha", "claude")
    assert alpha is not None
    assert alpha.installed_at == "2026-06-26T00:00:00Z"


def test_load_rejects_newer_schema_with_recovery_message(tmp_path):
    path = tmp_path / "state.json"
    path.write_text('{"schema_version": 2, "installs": []}\n', encoding="utf-8")

    with pytest.raises(StateError) as excinfo:
        State.load(path)

    message = str(excinfo.value)
    assert str(path) in message
    assert "newer my-skills" in message
    assert "upgrade the CLI" in message
    assert "move" in message
    assert "re-run install" in message


def test_load_rejects_records_missing_required_fields(tmp_path):
    path = tmp_path / "state.json"
    path.write_text(
        '{"schema_version": 1, "installs": [{"skill": "alpha"}]}\n',
        encoding="utf-8",
    )

    with pytest.raises(StateError) as excinfo:
        State.load(path)

    message = str(excinfo.value)
    assert str(path) in message
    assert "missing required field" in message
    assert "host" in message


def test_save_atomic_overwrite_leaves_no_tmp(tmp_path):
    path = tmp_path / "state.json"
    state = State(path)
    state.put(_rec("alpha"))
    state.save()

    state.remove("alpha", "claude")
    state.put(_rec("gamma"))
    state.save()

    again = State.load(path)
    assert set(again.installs) == {("gamma", "claude")}
    assert not path.with_name(path.name + ".tmp").exists()


def test_default_state_path_honors_xdg(monkeypatch, tmp_path):
    monkeypatch.setenv("XDG_STATE_HOME", str(tmp_path / "xdg"))
    assert default_state_path() == tmp_path / "xdg" / "my-skills" / "state.json"


def test_default_state_path_fallback(monkeypatch, tmp_path):
    monkeypatch.delenv("XDG_STATE_HOME", raising=False)
    monkeypatch.setenv("HOME", str(tmp_path))
    assert default_state_path() == tmp_path / ".local" / "state" / "my-skills" / "state.json"
