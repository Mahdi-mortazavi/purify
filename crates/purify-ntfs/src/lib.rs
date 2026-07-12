//! Raw NTFS / volume access for **purify**.
//!
//! # Why this crate exists
//!
//! Reading the NTFS Master File Table directly is what makes a full-drive scan
//! finish in seconds instead of minutes. Doing so requires opening a raw volume
//! handle (`\\.\C:`) — an operation that is Windows-specific.
//!
//! To honour purify's "minimize unsafe" principle, all such code is quarantined
//! in this one crate. NTFS structure *parsing* is delegated to the safe
//! [`ntfs`](https://crates.io/crates/ntfs) crate, so this crate currently
//! contains **no `unsafe` at all** — a raw read-only volume handle is obtained
//! through `std::fs::File`, and byte-granular reads are served through the
//! sector-aligning [`aligned::AlignedReader`]. Should future work need raw
//! `DeviceIoControl` calls, they will live here behind documented `// SAFETY:`
//! blocks; the crate manifest already relaxes `unsafe_code` to `deny` for that.
//!
//! The [`mft`] traversal is generic over `Read + Seek`, so it is unit-tested on
//! every platform against a real NTFS filesystem image.

// Test code legitimately uses unwrap/expect for assertions; production paths
// stay panic-free (enforced by the workspace lints outside `test`).
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod aligned;
pub mod mft;

use std::path::Path;

use purify_core::{FileEntry, Result, Scanner};

/// Typical NTFS volumes report a 512-byte logical sector; 4096 is also common
/// on Advanced Format drives. 4096 is a safe alignment for both (any multiple
/// of 512 that is also a multiple of 4096 satisfies both), so we align to it.
#[cfg(windows)]
const VOLUME_ALIGNMENT: u64 = 4096;

/// High-performance scanner backed by direct NTFS MFT reads.
///
/// On Windows it opens the volume that hosts the requested path and walks its
/// MFT. On other platforms direct volume access is unavailable and
/// [`Scanner::scan`] returns [`purify_core::Error::Unsupported`] so callers fall
/// back to the portable walker.
#[derive(Debug, Default)]
pub struct MftScanner {
    _private: (),
}

impl MftScanner {
    /// Create a new MFT scanner.
    #[must_use]
    pub fn new() -> Self {
        Self { _private: () }
    }

    /// Whether direct MFT scanning is available in this build.
    ///
    /// Returns `true` only on Windows. Callers should consult this and fall back
    /// to a portable [`Scanner`] when it returns `false`.
    #[must_use]
    pub fn is_available() -> bool {
        cfg!(windows)
    }

    /// Fast validation that the volume hosting `root` can be opened and parsed
    /// as NTFS, returning the number of user entries in its root directory.
    ///
    /// This exercises the exact raw-volume path used by [`Scanner::scan`]
    /// (open `\\.\X:`, sector-align, parse NTFS) without a slow full traversal —
    /// suitable for a smoke test. Requires administrator rights on Windows;
    /// returns [`purify_core::Error::Unsupported`] on other platforms.
    #[cfg(windows)]
    pub fn probe(root: &Path) -> Result<usize> {
        use std::fs::File;
        use std::io::BufReader;

        let volume = windows_volume::device_path_for(root).ok_or_else(|| {
            purify_core::Error::Unsupported(format!(
                "cannot determine NTFS volume for path: {}",
                root.display()
            ))
        })?;
        let file = File::open(&volume).map_err(|e| purify_core::Error::io(&volume, e))?;
        let aligned = aligned::AlignedReader::new(file, VOLUME_ALIGNMENT);
        let mut reader = BufReader::new(aligned);
        mft::probe_reader(&mut reader)
    }

    /// Non-Windows stub: direct volume access is unavailable.
    #[cfg(not(windows))]
    pub fn probe(_root: &Path) -> Result<usize> {
        Err(purify_core::Error::Unsupported(
            "direct MFT access requires Windows".to_string(),
        ))
    }
}

impl Scanner for MftScanner {
    #[cfg(windows)]
    fn scan(&self, root: &Path, sink: &mut dyn FnMut(FileEntry)) -> Result<()> {
        use std::fs::File;
        use std::io::BufReader;

        // Determine the volume device path (e.g. `\\.\C:`) from the drive letter
        // of `root`.
        let volume = windows_volume::device_path_for(root).ok_or_else(|| {
            purify_core::Error::Unsupported(format!(
                "cannot determine NTFS volume for path: {}",
                root.display()
            ))
        })?;

        let file = File::open(&volume).map_err(|e| purify_core::Error::io(&volume, e))?;
        let aligned = aligned::AlignedReader::new(file, VOLUME_ALIGNMENT);
        let mut reader = BufReader::new(aligned);

        // The MFT paths are volume-relative (starting at `\`). Rebase them onto
        // the drive prefix so downstream consumers get absolute paths.
        let prefix = windows_volume::drive_prefix(root);
        mft::scan_reader(&mut reader, |entry| {
            sink(rebase_entry(entry, &prefix));
        })
    }

    #[cfg(not(windows))]
    fn scan(&self, _root: &Path, _sink: &mut dyn FnMut(FileEntry)) -> Result<()> {
        Err(purify_core::Error::Unsupported(
            "direct MFT scanning requires Windows; use the portable walker".to_string(),
        ))
    }

    fn strategy_name(&self) -> &'static str {
        "ntfs-mft"
    }
}

/// Rebase a volume-relative entry path (e.g. `\Users\me\f.txt`) onto a drive
/// prefix (e.g. `C:`), producing `C:\Users\me\f.txt`.
#[cfg(windows)]
fn rebase_entry(mut entry: FileEntry, prefix: &str) -> FileEntry {
    let rel = entry.path.to_string_lossy();
    let rel = rel.trim_start_matches('\\');
    entry.path = std::path::PathBuf::from(format!("{prefix}\\{rel}"));
    entry
}

#[cfg(windows)]
mod windows_volume {
    use std::path::{Path, PathBuf};

    /// Extract the `C:` style drive prefix from a path, defaulting to `C:`.
    pub(crate) fn drive_prefix(path: &Path) -> String {
        let s = path.to_string_lossy();
        let bytes = s.as_bytes();
        if bytes.len() >= 2 && bytes[1] == b':' {
            format!("{}:", (bytes[0] as char).to_ascii_uppercase())
        } else {
            "C:".to_string()
        }
    }

    /// Build the raw volume device path `\\.\C:` for the drive hosting `path`.
    pub(crate) fn device_path_for(path: &Path) -> Option<PathBuf> {
        let s = path.to_string_lossy();
        let bytes = s.as_bytes();
        if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
            let letter = (bytes[0] as char).to_ascii_uppercase();
            Some(PathBuf::from(format!(r"\\.\{letter}:")))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn availability_matches_platform() {
        assert_eq!(MftScanner::is_available(), cfg!(windows));
    }

    #[test]
    fn strategy_name_is_stable() {
        assert_eq!(MftScanner::new().strategy_name(), "ntfs-mft");
    }

    #[cfg(not(windows))]
    #[test]
    fn scan_is_unsupported_off_windows() {
        let scanner = MftScanner::new();
        let mut count = 0;
        let result = scanner.scan(std::path::Path::new("."), &mut |_| count += 1);
        assert!(matches!(result, Err(purify_core::Error::Unsupported(_))));
        assert_eq!(count, 0);
    }

    // Real-hardware smoke test of the raw `\\.\C:` device path — the one code
    // path that cannot run on non-Windows hosts. GitHub's Windows runners have
    // administrator rights, so the volume opens; if a future runner lacks them,
    // we accept a permission error rather than failing spuriously, but never a
    // panic or a logic error.
    #[cfg(windows)]
    #[test]
    fn probe_opens_real_windows_volume() {
        let root = std::env::current_dir().expect("cwd");
        match MftScanner::probe(&root) {
            Ok(n) => assert!(n > 0, "root directory should contain entries"),
            Err(purify_core::Error::Io { .. }) => {
                eprintln!("skipping: volume open denied (no admin?)");
            }
            Err(e) => panic!("unexpected MFT probe error: {e}"),
        }
    }
}
