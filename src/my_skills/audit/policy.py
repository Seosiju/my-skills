from __future__ import annotations

from collections.abc import Iterable
from dataclasses import dataclass, field
from typing import Protocol

from .models import Severity, severity_from_text


PROFILE_THRESHOLDS: dict[str, Severity | None] = {
    "default": Severity.CRITICAL,
    "strict": Severity.HIGH,
    "permissive": None,
}


class AuditPolicyConfig(Protocol):
    @property
    def enabled(self) -> bool: ...

    @property
    def profile(self) -> str: ...

    @property
    def threshold(self) -> str | None: ...

    @property
    def disabled_rules(self) -> Iterable[str]: ...


@dataclass(frozen=True, slots=True)
class AuditPolicy:
    enabled: bool = True
    profile: str = "default"
    threshold: Severity | None = None
    disabled_rules: frozenset[str] = field(default_factory=frozenset)

    @property
    def effective_threshold(self) -> Severity | None:
        if self.threshold is not None:
            return self.threshold
        return PROFILE_THRESHOLDS.get(self.profile, Severity.CRITICAL)


def policy_from_config(config: AuditPolicyConfig | None) -> AuditPolicy:
    if config is None:
        return AuditPolicy()
    threshold = severity_from_text(config.threshold)
    return AuditPolicy(
        enabled=bool(config.enabled),
        profile=str(config.profile),
        threshold=threshold,
        disabled_rules=frozenset(config.disabled_rules),
    )
