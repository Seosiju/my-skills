"""Regenerate the v0.4.1 validation golden with uv run --locked python."""

import json
from pathlib import Path

from my_skills.hosts import all_hosts
from my_skills.validation import validate_skill, validate_skill_for_host


HERE = Path(__file__).resolve().parent
RUST_FIXTURES = HERE.parent.parent / "fixtures" / "validate"
PYTHON_FIXTURES = HERE.parents[1] / "fixtures" / "shared"
SHARED_FIXTURES = (
    "bad-name",
    "good-skill",
    "malformed-frontmatter",
    "missing-description",
    "missing-name",
    "missing-reference",
    "name-mismatch",
    "no-frontmatter",
    "unterminated-frontmatter",
)


def main():
    fixtures = [PYTHON_FIXTURES / name for name in SHARED_FIXTURES]
    fixtures.extend(sorted(path for path in RUST_FIXTURES.iterdir() if path.is_dir()))
    golden = {}
    for path in fixtures:
        results = {"standard": validate_skill(path)}
        for host in all_hosts():
            results[host.name] = validate_skill_for_host(path, host)
        golden[path.name] = {
            host: {"errors": result.errors, "warnings": result.warnings}
            for host, result in results.items()
        }
    payload = json.dumps(golden, ensure_ascii=False, indent=2, sort_keys=True)
    (HERE / "validation.json").write_text(payload + "\n", encoding="utf-8", newline="\n")


if __name__ == "__main__":
    main()
