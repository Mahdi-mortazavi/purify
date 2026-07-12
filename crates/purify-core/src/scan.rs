//! Filesystem scanning abstraction and the portable parallel walker.

use std::path::Path;

use crate::error::{Error, Result};
use crate::model::FileEntry;

/// Abstraction over a filesystem-scanning strategy.
///
/// The high-performance implementation reads the NTFS Master File Table
/// directly (`purify-ntfs`); [`WalkScanner`] is the portable fallback. Keeping
/// this a trait lets the rest of the engine stay platform-agnostic and fully
/// unit-testable.
pub trait Scanner {
    /// Scan `root`, invoking `sink` once per discovered [`FileEntry`].
    ///
    /// Streaming via a callback (rather than returning a `Vec`) keeps peak
    /// memory bounded on drives with tens of millions of files. Per-entry
    /// errors (e.g. permission denied on one file) are logged and skipped; only
    /// a failure to access `root` itself is returned as an error.
    fn scan(&self, root: &Path, sink: &mut dyn FnMut(FileEntry)) -> Result<()>;

    /// A short human-readable name for this strategy, used in CLI output.
    fn strategy_name(&self) -> &'static str;
}

/// Portable, parallel directory walker built on [`jwalk`].
///
/// Works on every platform and without administrator privileges. It is the
/// reliable default; the NTFS MFT scanner is an optional fast path on Windows.
#[derive(Debug, Clone, Default)]
pub struct WalkScanner {
    /// Whether to follow symbolic links. Off by default (the `Default`) to avoid
    /// cycles and double-counting — critical for correct disk-usage numbers.
    follow_links: bool,
    /// Whether to skip hidden files/dirs. Off by default (we want a complete
    /// picture of what fills the disk).
    skip_hidden: bool,
}

impl WalkScanner {
    /// Create a walker with default settings.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set whether symbolic links are followed.
    #[must_use]
    pub fn follow_links(mut self, yes: bool) -> Self {
        self.follow_links = yes;
        self
    }

    /// Set whether hidden entries are skipped.
    #[must_use]
    pub fn skip_hidden(mut self, yes: bool) -> Self {
        self.skip_hidden = yes;
        self
    }
}

impl Scanner for WalkScanner {
    fn scan(&self, root: &Path, sink: &mut dyn FnMut(FileEntry)) -> Result<()> {
        if !root.exists() {
            return Err(Error::TargetNotFound(root.to_path_buf()));
        }

        let walker = jwalk::WalkDir::new(root)
            .follow_links(self.follow_links)
            .skip_hidden(self.skip_hidden);

        for entry in walker {
            let entry = match entry {
                Ok(e) => e,
                Err(err) => {
                    // A single unreadable directory must not abort the scan.
                    tracing::warn!(%err, "skipping unreadable entry during walk");
                    continue;
                }
            };

            let path = entry.path();
            let file_type = entry.file_type();

            if file_type.is_dir() {
                sink(FileEntry::dir(path));
            } else if file_type.is_file() {
                // `metadata()` may fail for a file that vanished mid-scan or
                // that we lack permission to stat; skip it rather than abort.
                match entry.metadata() {
                    Ok(meta) => {
                        let modified = meta
                            .modified()
                            .ok()
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| d.as_secs() as i64);
                        sink(FileEntry::file(path, meta.len()).with_modified(modified));
                    }
                    Err(err) => {
                        tracing::warn!(path = %path.display(), %err, "skipping file with unreadable metadata");
                    }
                }
            }
            // Symlinks (when not following) and other special files are ignored:
            // they hold no meaningful "space on disk" for our purposes.
        }

        Ok(())
    }

    fn strategy_name(&self) -> &'static str {
        "portable-walk"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_file(path: &Path, bytes: usize) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(path, vec![0u8; bytes]).expect("write file");
    }

    #[test]
    fn walk_scanner_reports_files_and_dirs() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        write_file(&root.join("a.txt"), 100);
        write_file(&root.join("sub/b.bin"), 250);

        let scanner = WalkScanner::new();
        let mut files = 0u64;
        let mut total = 0u64;
        let mut dirs = 0u64;
        scanner
            .scan(root, &mut |e| {
                if e.is_dir {
                    dirs += 1;
                } else {
                    files += 1;
                    total += e.size;
                }
            })
            .expect("scan ok");

        assert_eq!(files, 2, "two files discovered");
        assert_eq!(total, 350, "sizes summed correctly");
        assert!(dirs >= 2, "root and sub directories discovered");
    }

    #[test]
    fn walk_scanner_missing_root_is_target_not_found() {
        let scanner = WalkScanner::new();
        let result = scanner.scan(Path::new("/no/such/path/here"), &mut |_| {});
        assert!(matches!(result, Err(Error::TargetNotFound(_))));
    }
}
