//! my-skills v1: personal skill drive.
//!
//! The spec is `docs/2026-11-23-skill-drive-rust-v1-spec.md`. Section numbers
//! in doc comments (for example "spec §6.1") refer to that document.
//!
//! Layout, by function (handoff §7 A):
//!
//! | module | role |
//! |---|---|
//! | `cli` | command-line definitions and dispatch |
//! | `commands` | one handler per command group: flow and output only |
//! | `registry` | the canonical side: root resolution, manifest, edits, catalog, seeds, share/import |
//! | `drive` | the connection side: hosts, links, planning, applying, state, content hash |
//! | `check` | Agent Skills validation and the security audit |
//! | `git` | registry git hooks |
//! | `update` | self-update from GitHub Releases |
//! | `platform` | OS paths and machine-local data directories |
//! | `error` | crate-wide error type |
//!
//! Stream ownership during the parallel build is recorded in
//! `docs/tasks/2026-11-23-rust-v1/` and the handoff document.

pub mod check;
pub mod cli;
pub mod commands;
pub mod drive;
pub mod error;
pub mod git;
pub mod platform;
pub mod registry;
pub mod update;

pub use error::{Error, Result};
