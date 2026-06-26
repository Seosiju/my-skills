import pytest

from my_skills.config import expand_path
from my_skills.hosts import HostConfig, all_hosts, get_host, host_names


def test_no_duplicate_names():
    names = host_names()
    assert len(names) == len(set(names))
    assert set(names) == {"claude", "codex", "hermes"}


def test_get_host_unknown_raises():
    with pytest.raises(KeyError):
        get_host("nope")


def test_get_host_returns_config():
    assert get_host("claude").display_name == "Claude Code"


# The "four host config pass common validation" completion evidence (plan 17).
@pytest.mark.parametrize("host", all_hosts(), ids=lambda h: h.name)
def test_host_contract(host: HostConfig):
    assert host.name and host.name.islower()
    assert host.display_name
    assert host.detect_commands and all(host.detect_commands)
    assert host.default_user_path
    assert expand_path(host.default_user_path).is_absolute()
    assert host.default_project_path
    assert isinstance(host.supports_symlink, bool)
    assert host.reload_hint
