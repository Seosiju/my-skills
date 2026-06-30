from __future__ import annotations

from dataclasses import dataclass, field

from .models import Severity, severity_from_text


PROFILE_THRESHOLDS: dict[str, Severity | None] = {
    "default": Severity.CRITICAL,
    "strict": Severity.HIGH,
    "permissive": None,
}


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


def policy_from_config(config) -> AuditPolicy:
    threshold = severity_from_text(getattr(config, "threshold", None))
    return AuditPolicy(
        enabled=bool(getattr(config, "enabled", True)),
        profile=str(getattr(config, "profile", "default")),
        threshold=threshold,
        disabled_rules=frozenset(getattr(config, "disabled_rules", [])),
    )
