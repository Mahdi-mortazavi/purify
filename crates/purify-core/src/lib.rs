//! Core engine for **purify** — the pure-Rust, platform-agnostic heart of the
//! tool.
//!
//! This crate deliberately contains **no** `unsafe` and **no** OS-specific
//! filesystem access. Raw NTFS/volume access lives behind the [`Scanner`] trait
//! and is implemented in the `purify-ntfs` crate. Everything here can be unit
//! tested against a synthetic filesystem without touching a real disk.
//!
//! At Phase 0 this crate only defines the shared vocabulary of the project
//! (error type, byte helpers, and the scanning trait). Later phases fill in the
//! rule engine, deduplication, and quarantine logic.

use std::path::PathBuf;

/// Errors surfaced by the core engine.
///
/// Library code returns typed errors via `thiserror`; binaries are free to wrap
/// these in `anyhow` for reporting. We never `panic!` on a production path.
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
}

/// Convenience alias for results produced by the core engine.
pub type Result<T> = std::result::Result<T, Error>;

/// A single entry discovered by a [`Scanner`].
///
/// Kept intentionally small and `Copy`-free so it can represent millions of
/// files without excessive memory pressure during a full-drive scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    /// Absolute path to the file.
    pub path: PathBuf,
    /// Logical size in bytes.
    pub size: u64,
    /// Whether the entry is a directory.
    pub is_dir: bool,
}

/// Abstraction over a filesystem-scanning strategy.
///
/// The high-performance implementation reads the NTFS Master File Table
/// directly (`purify-ntfs`); a portable implementation walks the directory tree
/// (added in Phase 1). Keeping this a trait lets the core engine and CLI stay
/// completely platform-agnostic and fully unit-testable.
pub trait Scanner {
    /// Scan `root`, invoking `sink` once per discovered [`FileEntry`].
    ///
    /// Streaming via a callback (rather than returning a `Vec`) keeps peak
    /// memory bounded on drives with tens of millions of files.
    fn scan(&self, root: &std::path::Path, sink: &mut dyn FnMut(FileEntry)) -> Result<()>;
}

/// Format a byte count into a human-readable string using binary (IEC) units.
///
/// Disk sizes are conventionally reported in powers of 1024 (KiB/MiB/GiB), so
/// this matches what users see in Windows Explorer's "size on disk".
#[must_use]
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    if bytes == 0 {
        return "0 B".to_string();
    }
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.2} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_bytes_handles_zero() {
        assert_eq!(format_bytes(0), "0 B");
    }

    #[test]
    fn format_bytes_stays_in_bytes_below_a_kib() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1023), "1023 B");
    }

    #[test]
    fn format_bytes_scales_to_binary_units() {
        assert_eq!(format_bytes(1024), "1.00 KiB");
        assert_eq!(format_bytes(1024 * 1024), "1.00 MiB");
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.00 GiB");
    }

    #[test]
    fn format_bytes_rounds_to_two_decimals() {
        assert_eq!(format_bytes(1536), "1.50 KiB");
    }
}
