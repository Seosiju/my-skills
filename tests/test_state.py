from my_skills.state import InstallRecord, State, default_state_path


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
    assert again.get("alpha", "claude").mode == "copy"


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
