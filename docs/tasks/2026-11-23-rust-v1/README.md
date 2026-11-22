# Rust v1 phase 1: parallel streams, common rules

Spec: `docs/2026-11-23-skill-drive-rust-v1-spec.md` (read §0, §4, §5, §8, §10 first).
Contract: `crates/my-skills/src/lib.rs` and the module you own.

| Stream | Brief | Owner | Branch | Worktree |
|---|---|---|---|---|
| A hash | `stream-a-hash.md` | Codex | `dev-stream-a` | `<worktrees>/stream-a` |
| B manifest/state/data | `stream-b-manifest-state.md` | Codex | `dev-stream-b` | `<worktrees>/stream-b` |
| C link | (integrator) | Claude | `dev` | main checkout |
| D frontmatter/validate | `stream-d-validate.md` | Codex | `dev-stream-d` | `<worktrees>/stream-d` |
| E audit | `stream-e-audit.md` | Codex | `dev-stream-e` | `<worktrees>/stream-e` |
| F CI/release | `stream-f-ci-release.md` | Codex | `dev-stream-f` | `<worktrees>/stream-f` |

## Rules for every stream

1. **Stay in your worktree.** Edit only the files your brief lists. Do not edit
   another stream's modules, `lib.rs`, `cli.rs`, `error.rs`, `paths.rs`, `hosts.rs`,
   or `ignore.rs`.
2. **Keep the contract.** Do not change public types or signatures that the
   skeleton declares. You may add private items, private submodules, and new
   public items inside your own module. If a contract change is really needed,
   work around it inside your module and record the request in your report.
3. **Never touch real user state.** Do not read or write the user's registry,
   `~/.claude/skills`, `~/.agents/skills`, `~/.gemini`, `~/.hermes`,
   `~/.local/state/my-skills`, `~/.local/share/my-skills`, or `~/.config/my-skills`.
   Tests use `tempfile` and set `HOME`, `XDG_STATE_HOME`, `XDG_DATA_HOME`,
   `XDG_CONFIG_HOME` to temporary directories.
4. **Public repository hygiene.** Fixtures are synthetic. No real usernames, home
   paths, emails, tokens, or the user's skill names.
5. **Parity source.** v0.4.1 Python lives in `src/my_skills/`. Read it, run it to
   produce golden data (`uv run python ...`), and do not modify it.
6. **Dependencies.** Add crates only in your own block of
   `crates/my-skills/Cargo.toml`, as caret requirements. YAML is `yaml-rust2`
   (already in the shared block); do not add another YAML crate.
7. **Windows must compile.** Code must build for `x86_64-pc-windows-msvc` even
   though this Mac cannot run it. Put OS-specific code behind `cfg`.
8. **Checks before you finish** (the `cargo fmt` / `cargo clippy` subcommands are
   broken on this Mac by rustup proxies in `~/.cargo/bin`; call the tools directly):

   ```bash
   cargo build
   cargo test
   rustfmt --edition 2024 $(git ls-files 'crates/*.rs') <your new .rs files>
   /opt/homebrew/bin/cargo-clippy clippy --all-targets -- -D warnings
   ```

   `todo!()` bodies in other streams are expected; your tests must not call them.
   A test that cannot run until another stream lands is marked
   `#[ignore = "needs stream X: <what>"]`.
9. **Git.** Commit on your branch in small logical commits with messages like
   `feat(rust/a): ...`. Do not push, tag, merge, or rebase other branches.
10. **Report.** Finish by writing `docs/tasks/2026-11-23-rust-v1/reports/<stream>.md`
    and committing it. Use these sections: Done, Tests (commands and results),
    Differences from v0.4.1 (each one deliberate or not), Contract change
    requests, Open issues. Then stop.
