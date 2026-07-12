//! Shared data model for scanned filesystem entries.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// A single entry discovered by a scanner.
///
/// Kept intentionally small so it can represent millions of files without
/// excessive memory pressure during a full-drive scan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    /// Absolute path to the file or directory.
    pub path: PathBuf,
    /// Logical size in bytes (0 for directories).
    pub size: u64,
    /// Whether the entry is a directory.
    pub is_dir: bool,
    /// Last-modified time as a Unix timestamp in seconds, if known.
    ///
    /// Populated by the portable walker; the MFT scanner leaves it `None` for
    /// now, so age-based rules currently rely on the walker.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified: Option<i64>,
}

impl FileEntry {
    /// Convenience constructor for a file entry.
    #[must_use]
    pub fn file(path: impl Into<PathBuf>, size: u64) -> Self {
        Self {
            path: path.into(),
            size,
            is_dir: false,
            modified: None,
        }
    }

    /// Convenience constructor for a directory entry.
    #[must_use]
    pub fn dir(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            size: 0,
            is_dir: true,
            modified: None,
        }
    }

    /// Set the last-modified timestamp (builder style).
    #[must_use]
    pub fn with_modified(mut self, modified: Option<i64>) -> Self {
        self.modified = modified;
        self
    }

    /// The file extension in lowercase, if any (without the leading dot).
    #[must_use]
    pub fn extension_lower(&self) -> Option<String> {
        self.path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
    }

    /// The final path component (file or directory name) as a lossy string.
    #[must_use]
    pub fn file_name_lossy(&self) -> String {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
}
