# Stream D — Frontmatter and validation

## Done

- Ported frontmatter splitting, standard skill validation, and host-specific
  validation. Standard error order, warning order, and the decision to skip
  host checks after a standard error match the Python results in the fixtures.
- Used `yaml-rust2` events to construct values with PyYAML scalar and duplicate
  key behavior. Preserved mapping insertion order for Python-style string
  conversion of nested values.
- Added a v0.4.1 golden generator and tested 68 synthetic fixtures, including
  nine shared Python fixtures. The comparison covers standard validation and
  all four built-in hosts, for 340 exact error and warning comparisons.
- Kept audit composition with the integrator. Changes are local to
  `dev-stream-d`.

## Tests

| Command | Result |
| --- | --- |
| `cargo build` | Passed on macOS. |
| `cargo test` | 10 passed, including six stream D tests. |
| `rustfmt --edition 2024 $(git ls-files 'crates/*.rs') crates/my-skills/tests/{frontmatter_parity,validate_golden}.rs` | Passed. A subsequent `rustfmt --check` passed on the same files. |
| `/opt/homebrew/bin/cargo-clippy clippy --all-targets -- -D warnings` | Passed. |
| `cargo check --locked --target x86_64-pc-windows-msvc --all-targets` | Passed with an isolated Rust 1.95 toolchain. |
| `cargo build --locked --target x86_64-pc-windows-msvc --lib` | Passed. |
| `uv run --locked python crates/my-skills/tests/golden/validate/generate.py` | Passed. Regeneration produced identical JSON bytes. |
| `uv run --locked pytest tests/test_frontmatter.py tests/test_validation.py` | 31 passed. |
| `git diff --cached --check` | Passed for the staged implementation and tests. |

Rust and Python checks ran with temporary HOME, XDG state, data, and config
directories. Validation tests use only synthetic files copied into temporary
directories.

## Differences from v0.4.1

### YAML 1.1 compatibility decisions

- **Booleans.** Plain `yes`/`no` and `on`/`off` use PyYAML's case spellings and
  truthiness. A true value stringifies as `True`, and a false required value
  produces a missing-field error. Ordinary quoted scalars stay strings.
- **Octal.** Plain `0123` resolves to decimal `83`. YAML 1.2's `0o123` spelling
  and invalid implicit octal such as `019` stay strings. Zero is falsey.
- **Timestamps.** Date and datetime scalars retain their type internally for
  truthiness, root type errors, and Python string conversion. The public
  `Value` exposes their normalized text because its variants have no date type.
- **Duplicate keys.** The last value wins. The key retains its first insertion
  position. Explicit entries override YAML merge entries, and earlier mappings
  in a merge sequence take precedence.

### Other differences

- **Deliberate.** Oversized Python integers retain exact decimal text in
  validation and use strings in the public `Value`. Public mappings stringify
  non-string keys. Host validation produces an unsupported-field warning for
  such keys, where Python can raise a `TypeError`.
- **Deliberate.** An unreadable or non-UTF-8 `SKILL.md` produces a validation
  error. The Python file read can raise an exception.
- **Deliberate.** Recursive aliases and collection nesting deeper than 128
  produce a frontmatter error. The public value type represents finite trees.
  YAML sets render in insertion order. Python set order can vary by process.
- **Remaining parity gap.** Some scanner and constructor errors fall back to
  `yaml-rust2` text, and the two parsers accept different inputs at uncommon
  YAML grammar edges. Python exceptions for values such as invalid calendar
  dates are outside the golden comparison.

## Contract change requests

The existing public signatures and types were sufficient for this stream.

## Open issues

- Extend the differential YAML corpus if production skills expose a diagnostic
  or grammar difference. The golden fixture comparison covers the listed
  rules and syntax cases.
- Windows library compilation and all-target checking passed on macOS.
  Windows test execution and binary linking remain for Windows CI.
