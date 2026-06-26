"""Host registry — the single lookup point for host configuration."""

from __future__ import annotations

from .base import HostConfig
from .claude import CLAUDE
from .codex import CODEX
from .hermes import HERMES

_HOSTS: dict[str, HostConfig] = {
    host.name: host for host in (CLAUDE, CODEX, HERMES)
}


def all_hosts() -> list[HostConfig]:
    """Return every registered host config in registration order."""
    return list(_HOSTS.values())


def host_names() -> list[str]:
    """Return the registered host names."""
    return list(_HOSTS)


def get_host(name: str) -> HostConfig:
    """Look up a host by name, raising ``KeyError`` if it is unknown."""
    try:
        return _HOSTS[name]
    except KeyError:
        raise KeyError(f"unknown host: {name!r}") from None


__all__ = ["HostConfig", "all_hosts", "host_names", "get_host"]
