from pathlib import Path

from my_skills.audit.analyzers import run_audit
from my_skills.audit.policy import AuditPolicy
from my_skills.checks import compose_validation

REPO_ROOT = Path(__file__).resolve().parents[1]


def test_repo_skills_compose_validation_and_audit_smoke():
    skill_dirs = tuple(
        sorted(path.parent for path in (REPO_ROOT / "skills").glob("*/SKILL.md"))
    )
    assert skill_dirs

    for skill_dir in skill_dirs:
        validation = compose_validation(skill_dir)
        audit = run_audit(skill_dir, policy=AuditPolicy())

        assert validation.ok, f"{skill_dir.name}: {validation.errors}"
        assert not audit.blocked, f"{skill_dir.name}: {audit.findings}"
