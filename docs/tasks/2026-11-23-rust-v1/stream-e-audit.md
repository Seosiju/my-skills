# Stream E: audit

Read `README.md` in this folder first.

## Goal

Port v0.4.1 `src/my_skills/audit/` (about 780 lines plus `builtin_rules.yaml`) to
`crates/my-skills/src/audit/`, with identical findings, ordering, policy, and
result hashes.

## Files you own

- `crates/my-skills/src/audit/` (`mod.rs` contract plus any private submodules,
  such as `rules.rs`, `analyzers.rs`, `markdown.rs`, `dataflow.rs`,
  `structure.rs`, `static_checks.rs`, `bundle.rs`, `formatting.rs`)
- `crates/my-skills/src/audit/builtin_rules.yaml`: a copy of v0.4.1's file,
  embedded with `include_str!`
- `crates/my-skills/tests/audit_*.rs`
- `crates/my-skills/tests/fixtures/audit/`, `tests/golden/audit/`

## Requirements

1. Port every analyzer and rule in v0.4.1 `audit/`: `analyzers.py`, `bundle.py`,
   `dataflow.py`, `markdown.py`, `static.py`, `structure.py`, `policy.py`, and
   `models.py`. Each finding must match v0.4.1 in `rule_id`, `category`,
   `severity`, `file`, `message`, `line`, and order.
2. `policy_from_settings` matches `policy.py`, including the profile defaults and
   `effective_threshold`. An unknown severity text is an error with the v0.4.1
   message.
3. `AuditResult::result_hash` must produce the same bytes as v0.4.1:
   `json.dumps(payload, sort_keys=True, separators=(",", ":"))`. **Python's
   default `ensure_ascii=True` escapes non-ASCII as `\uXXXX`, with surrogate
   pairs above U+FFFF.** Reproduce that exactly, and test it with a Korean
   message and an emoji.
4. Parse the rules YAML with `yaml-rust2`. Note any PyYAML 1.1 versus YAML 1.2
   difference that affects the rules file. There should be none; say so if true.
5. Formatting: add public `format_gate(&AuditGateResult, blocked_title: &str) -> String`
   and `gate_json(&AuditGateResult) -> serde_json::Value`. They mirror v0.4.1
   `audit/formatting.py`. The integrator calls them from the CLI.
6. Keep the regex semantics identical. If you add `regex`, check each pattern
   for Python-only syntax (lookaround, backreferences). Rewrite those patterns
   so the matches stay the same, and list them in the report.

## Golden data

Write a generator, run with `uv run python`. It runs v0.4.1 `run_audit` with the
default policy and with each profile over every fixture. It writes the findings
and `result_hash` to `tests/golden/audit/golden.json`.

Fixtures:
- reuse the synthetic audit fixtures from the repo's `tests/fixtures/`
- add one per rule that fires
- add clean skills that produce no findings

## Done when

- All checks in README rule 8 pass.
- Every rule in `builtin_rules.yaml` and every analyzer has a fixture that
  triggers it, and the golden results match.
- The report lists any rule whose behavior differs, with the reason.
