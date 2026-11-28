# Stream E audit report

## Done

- Ported static, structure, Markdown, command-tier, and dataflow analyzers in
  v0.4.1 order. Ported cross-skill bundle findings for the multi-skill CLI.
- Implemented policy parsing, profile thresholds, analyzer and rule suppression,
  gate output, and Python-compatible finding hashes. Exposed
  `cross_skill_findings`, `format_gate`, `gate_json`, `finding_json`, and
  `result_json` for CLI integration.
- Embedded a byte-for-byte copy of `builtin_rules.yaml` with `yaml-rust2`.
  PyYAML 1.1 and YAML 1.2 resolve every scalar in this file to the same value.
  The direct `regex` dependency reuses version 1.13.2 from the workspace
  dependency graph. Both crates use MIT or Apache-2.0 licensing.
  [regex](https://docs.rs/crate/regex/1.13.2)
  [yaml-rust2](https://docs.rs/crate/yaml-rust2/0.13.2)
- Committed the Python generator, 19 new synthetic fixtures, and golden
  outputs for 30 fixtures under the implicit default and all three named
  profiles. The fixtures cover all 15 built-in rules, bundle ordering, scanner
  and encoding findings, clean skills, gate output, and hashes with Korean text,
  emoji, and control characters.

## Tests

| Command | Result |
| --- | --- |
| `uv run --locked python tests/golden/audit/generate.py` | v0.4.1 generated the golden JSON under Python 3.13.15. A second run reproduced the same bytes. |
| `uv run --locked pytest tests/test_audit.py tests/test_audit_static.py` | 23 passed. |
| `cargo build` | Passed on macOS. |
| `cargo test` | 13 passed, including all 9 audit tests. |
| `rustfmt --edition 2024 --check` over all tracked and new Rust files | Passed. Direct formatting was applied to Stream E files. |
| `/opt/homebrew/bin/cargo-clippy clippy --all-targets -- -D warnings` | Passed. |
| `cargo check --all-targets --target x86_64-pc-windows-msvc` with the Rust 1.96.0 toolchain | Passed from macOS, including Windows test compilation. |
| `git diff --cached --check` | Passed for both implementation and parity commits. |

Rust and Python checks used temporary home and XDG directories for audit
execution. All fixture paths were inside the tool repo or temporary directories.

## Differences from v0.4.1

- All built-in rule IDs, categories, severities, files, messages, lines, and
  ordering match the fixture results. Golden hashes use Python's compact
  sorted-key JSON, including ASCII escaping and UTF-16 surrogate pairs.
- Deliberate regex translation. `path-traversal` replaces Python's negative
  lookbehind `(?<![\w/])` with a preceding-character check on each `../`
  candidate. The patterns contain zero backreferences. Static, Markdown, and
  command patterns extend `\s` for Python's U+001C–U+001F whitespace.
  Static and command patterns using `\b` normalize Rust's extra word
  characters and Python's additional word characters before matching.
  ASCII case-insensitive patterns also account for Python's two dotted and
  dotless I variants. [Rust regex syntax](https://docs.rs/regex/1.13.2/regex/#syntax)
- Unicode classification can vary across Python and Rust Unicode database
  releases. The checked regex dependency uses Unicode 16.0 and the generator
  used Unicode 15.1. Newly classified code points can affect
  `path-traversal`, `aws-key`, `secret`, `prompt-injection`, `output-suppression`,
  `destructive-command`, `suspicious-fetch`, `credential-reader`, and
  `network-sender` where they rely on word or digit classification.
  Command matches can in turn affect dataflow and bundle findings.
- Deliberate filesystem snapshot reuse. File analyzers share one sorted
  path listing and one set of file reads. Dataflow reopens text files and
  applies Python's universal newline handling. Concurrent changes during a scan
  can yield different findings from Python's separate directory walks.
- Platform limits. Non-UTF-8 filenames use Rust's lossy path rendering.
  POSIX double-leading slashes are normalized in reported root paths.
  Finding line numbers saturate at the contract's `u32::MAX`. Windows maps
  common read errors to Python errno text and uses a generic I/O diagnosis for
  other error kinds.
- Deliberate API scope. The Rust API uses the fixed analyzer set defined by the existing
  `run_audit` contract. Policy suppression still applies by analyzer ID
  and by rule ID.

## Contract change requests

None.

## Open issues

- Integration owns the workspace `Cargo.lock` refresh. Cargo generated a
  one-line update adding the already-resolved `regex` crate to the
  `my-skills` dependency list during verification.
- The integrator can wire the audit formatters and cross-skill findings into
  the CLI. Windows runtime verification remains with the Windows test run.
