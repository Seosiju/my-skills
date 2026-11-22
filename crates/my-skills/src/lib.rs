//! my-skills v1: personal skill drive.
//!
//! The spec is `docs/2026-11-23-skill-drive-rust-v1-spec.md`. Section numbers
//! in doc comments (for example "spec §6.1") refer to that document.
//!
//! Module ownership during the parallel build (spec §9, phase 1):
//!
//! | module                    | stream | status            |
//! |---------------------------|--------|-------------------|
//! | `error`, `paths`, `ignore`, `hosts`, `cli` | skeleton | implemented |
//! | `hash`                    | A      | implemented       |
//! | `manifest`, `state`, `data` | B    | implemented       |
//! | `link`                    | C      | implemented       |
//! | `frontmatter`, `validate` | D      | implemented       |
//! | `audit`                   | E      | implemented       |
//! | `plan`, `install`, `runtime`, `commands::{connect,inspect}` | integrator | |
//! | `hooks`, `commands::{hooks,doctor}` | G | phase 2 |
//! | `manifest_edit`, `catalog`, `sharing`, `seeds`, `commands::registry` | H | phase 2 |
//! | `self_update`, `commands::update` | I | phase 2 |
//!
//! A stream may change function bodies and add private items inside its own
//! modules. It must not change the public signatures or types declared here
//! without the integrator's agreement, because other streams build on them.

pub mod audit;
pub mod catalog;
pub mod cli;
pub mod commands;
pub mod data;
pub mod error;
pub mod frontmatter;
pub mod hash;
pub mod hooks;
pub mod hosts;
pub mod ignore;
pub mod install;
pub mod link;
pub mod manifest;
pub mod manifest_edit;
pub mod paths;
pub mod plan;
pub mod runtime;
pub mod seeds;
pub mod self_update;
pub mod sharing;
pub mod state;
pub mod validate;

pub use error::{Error, Result};
