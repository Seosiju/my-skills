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
//! | `hash`                    | A      | contract only     |
//! | `manifest`, `state`, `data` | B    | contract only     |
//! | `link`                    | C      | contract only     |
//! | `frontmatter`, `validate` | D      | contract only     |
//! | `audit`                   | E      | contract only     |
//!
//! A stream may change function bodies and add private items inside its own
//! modules. It must not change the public signatures or types declared here
//! without the integrator's agreement, because other streams build on them.

pub mod audit;
pub mod cli;
pub mod data;
pub mod error;
pub mod frontmatter;
pub mod hash;
pub mod hosts;
pub mod ignore;
pub mod link;
pub mod manifest;
pub mod paths;
pub mod state;
pub mod validate;

pub use error::{Error, Result};
