from __future__ import annotations

from typing import NotRequired, TypedDict

from .audit.formatting import AuditGateJson
from .planner import PlanItem


class InstallActionJson(TypedDict):
    skill: str
    host: str
    action: str
    reason: str
    mode: str
    source: str
    destination: str


class InstallPlanJson(TypedDict):
    actions: list[InstallActionJson]
    audit: NotRequired[AuditGateJson]


def install_plan_json(plan: list[PlanItem]) -> InstallPlanJson:
    return {
        "actions": [
            {
                "skill": item.skill,
                "host": item.host,
                "action": item.action.value,
                "reason": item.reason,
                "mode": item.mode,
                "source": str(item.source),
                "destination": str(item.destination),
            }
            for item in plan
        ]
    }
