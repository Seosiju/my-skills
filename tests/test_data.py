import pytest

from my_skills import cli
from my_skills.data import data_root, skill_data_path

APP = "my-skills"


# --------------------------------------------------------------- data_root ---


def test_data_root_honors_xdg(monkeypatch, tmp_path):
    monkeypatch.setattr("sys.platform", "linux")
    monkeypatch.setenv("XDG_DATA_HOME", str(tmp_path / "xdg"))
    assert data_root() == tmp_path / "xdg" / APP


def test_data_root_posix_fallback(monkeypatch, tmp_path):
    monkeypatch.setattr("sys.platform", "linux")
    monkeypatch.delenv("XDG_DATA_HOME", raising=False)
    monkeypatch.setenv("HOME", str(tmp_path))
    assert data_root() == tmp_path / ".local" / "share" / APP


def test_data_root_windows(monkeypatch, tmp_path):
    monkeypatch.setattr("sys.platform", "win32")
    monkeypatch.setenv("LOCALAPPDATA", str(tmp_path / "AppData" / "Local"))
    # Windows keeps an extra 'data' segment (plan 15.4).
    assert data_root() == tmp_path / "AppData" / "Local" / APP / "data"


# --------------------------------------------------------- skill_data_path ---


def test_skill_data_path_appends_skill(monkeypatch, tmp_path):
    monkeypatch.setattr("sys.platform", "linux")
    monkeypatch.setenv("XDG_DATA_HOME", str(tmp_path))
    assert skill_data_path("personal-profile") == tmp_path / APP / "personal-profile"


def test_skill_data_path_create_false_does_not_touch_fs(monkeypatch, tmp_path):
    monkeypatch.setattr("sys.platform", "linux")
    monkeypatch.setenv("XDG_DATA_HOME", str(tmp_path))
    path = skill_data_path("memo")
    assert not path.exists()


def test_skill_data_path_create_true_makes_dir(monkeypatch, tmp_path):
    monkeypatch.setattr("sys.platform", "linux")
    monkeypatch.setenv("XDG_DATA_HOME", str(tmp_path))
    path = skill_data_path("memo", create=True)
    assert path.is_dir()
    # Idempotent.
    assert skill_data_path("memo", create=True) == path


@pytest.mark.parametrize("bad", ["", "..", "a/b", "a\\b", "../evil", "UPPER", "-lead"])
def test_skill_data_path_rejects_bad_names(bad):
    with pytest.raises(ValueError):
        skill_data_path(bad)


def test_bad_name_cannot_escape_root(monkeypatch, tmp_path):
    monkeypatch.setattr("sys.platform", "linux")
    monkeypatch.setenv("XDG_DATA_HOME", str(tmp_path))
    with pytest.raises(ValueError):
        skill_data_path("../../etc")


# ----------------------------------------------------------------- CLI ---


def test_cli_data_path_prints(monkeypatch, tmp_path, capsys):
    monkeypatch.setattr("sys.platform", "linux")
    monkeypatch.setenv("XDG_DATA_HOME", str(tmp_path))
    rc = cli.main(["data-path", "personal-profile"])
    out = capsys.readouterr().out.strip()
    assert rc == 0
    assert out == str(tmp_path / APP / "personal-profile")


def test_cli_data_path_create(monkeypatch, tmp_path, capsys):
    monkeypatch.setattr("sys.platform", "linux")
    monkeypatch.setenv("XDG_DATA_HOME", str(tmp_path))
    rc = cli.main(["data-path", "memo", "--create"])
    capsys.readouterr()
    assert rc == 0
    assert (tmp_path / APP / "memo").is_dir()


def test_cli_data_path_invalid_name_exit_2(monkeypatch, tmp_path, capsys):
    monkeypatch.setattr("sys.platform", "linux")
    monkeypatch.setenv("XDG_DATA_HOME", str(tmp_path))
    rc = cli.main(["data-path", "../evil"])
    err = capsys.readouterr().err
    assert rc == 2
    assert "invalid skill name" in err
