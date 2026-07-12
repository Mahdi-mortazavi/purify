//! Error types for the core engine.
//!
//! Library code returns these typed errors via `thiserror`; binaries are free
//! to wrap them in `anyhow` for reporting. We never `panic!` on a production
//! path.

use std::path::PathBuf;

/// Errors surfaced by the core engine.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// An I/O operation failed while scanning or moving files.
    #[error("i/o error at {path}: {source}")]
    Io {
        /// The path being operated on when the failure occurred.
        path: PathBuf,
        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// The requested scan target does not exist or is not accessible.
    #[error("scan target not found or inaccessible: {0}")]
    TargetNotFound(PathBuf),

    /// A feature is not available on the current platform (e.g. direct MFT
    /// reads on non-Windows). Callers should fall back to a portable strategy.
    #[error("unsupported on this platform: {0}")]
    Unsupported(String),

    /// A cleanup signature or rule file failed to parse.
    #[error("invalid signature file {path}: {message}")]
    InvalidSignature {
        /// Path to the offending signature file.
        path: PathBuf,
        /// Human-readable description of the problem.
        message: String,
    },

    /// A quarantine or metadata-store operation failed.
    #[error("quarantine error: {0}")]
    Quarantine(String),

    /// A refused operation that would have violated a safety invariant
    /// (e.g. attempting to quarantine a protected system path).
    #[error("refused for safety: {0}")]
    Refused(String),

    /// Parsing an NTFS on-disk structure failed.
    #[error("ntfs error: {0}")]
    Ntfs(String),
}

impl Error {
    /// Build an [`Error::Io`] from a path and an [`std::io::Error`].
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}

/// Convenience alias for results produced by the core engine.
pub type Result<T> = std::result::Result<T, Error>;
