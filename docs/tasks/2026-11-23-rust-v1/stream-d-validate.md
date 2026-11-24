# Stream D: frontmatter and validation

Read `README.md` in this folder first.

## Goal

Port v0.4.1 `src/my_skills/frontmatter.py` and `src/my_skills/validation.py` to
`frontmatter.rs` and `validate.rs`, with identical checks and message text.

## Files you own

- `crates/my-skills/src/frontmatter.rs`, `validate.rs`
- `crates/my-skills/tests/frontmatter_*.rs`, `validate_*.rs`
- `crates/my-skills/tests/fixtures/validate/`
- `crates/my-skills/tests/golden/validate/` (golden data and its generator)

## Requirements

1. Use `yaml-rust2` for the YAML. v0.4.1 uses PyYAML `safe_load` (YAML 1.1). Where
   the two differ, match PyYAML for every value validation looks at: `name`,
   `description`, and the other keys `validation.py` reads. Cases where they
   differ:
   - `yes`/`no`/`on`/`off` booleans
   - octal-looking numbers
   - timestamps
   - duplicate keys

   List each case in the report with the behavior you chose.
2. `parse_frontmatter` handles the same delimiters, BOM, CRLF line endings, an
   empty frontmatter block, and missing frontmatter as `frontmatter.py`. Each
   `FrontmatterError` message is the same text.
3. `validate_name`, `validate_skill`, and `validate_skill_for_host` produce the
   same `errors` and `warnings` strings in the same order as v0.4.1. That
   includes the Markdown link checks that use `_LINK_RE`, the name length and
   character rules, and the host's `description_max_chars`.
4. Do not call audit. v0.4.1 `checks.compose_validation` mixes in audit
   findings. The integrator wires that later.

## Golden data

Write a generator (standard library plus the repo's v0.4.1 package, run with
`uv run python`). It runs v0.4.1 `validate_skill` and `validate_skill_for_host`
for every built-in host over each fixture skill. It writes
`{fixture: {host-or-"standard": {"errors": [...], "warnings": [...]}}}`.
Reuse synthetic fixtures from `tests/fixtures/` where suitable, and add your own
for each rule. The Rust tests compare against this file.

## Done when

All checks in README rule 8 pass, every validation rule has at least one
failing fixture and one passing fixture, and the report lists the YAML 1.1/1.2
decisions.
