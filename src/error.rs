//! Crate-wide error type.

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// `my-skills.toml` / `my-skills.local.toml` missing or invalid.
    #[error("{0}")]
    Manifest(String),

    /// `state.json` missing fields, malformed, or written by a newer CLI.
    #[error("{0}")]
    State(String),

    /// A plan item could not persist its install record.
    #[error(transparent)]
    StateSave(Box<Error>),

    /// A managed copy changed after planning and was preserved.
    #[error("CONFLICT: {0} changed locally after planning")]
    CopyConflict(String),

    /// A link could not be created, read, or removed safely (spec §6.1).
    #[error("{0}")]
    Link(String),

    /// An operation was refused to protect an invariant (spec §4).
    /// The message names the invariant, for example "I2: ...".
    #[error("refused: {0}")]
    Refused(String),

    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

impl Error {
    /// Attach the path that an I/O call was working on.
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Error::Io {
            path: path.into(),
            source,
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
