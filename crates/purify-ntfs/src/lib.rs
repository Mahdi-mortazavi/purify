//! Raw NTFS / volume access for **purify**.
//!
//! # Why this crate exists
//!
//! Reading the NTFS Master File Table directly is what makes a full-drive scan
//! finish in seconds instead of minutes (this is how WizTree is fast). Doing so
//! requires opening a raw volume handle (`\\.\C:`) and reading sectors — an
//! operation that is fundamentally `unsafe` on Windows.
//!
//! To honour purify's "minimize unsafe" principle, **all** such code is
//! quarantined in this one crate. Every other crate in the workspace keeps
//! `#![forbid(unsafe_code)]`. Within this crate, `unsafe_code` is set to `deny`,
//! so each individual `unsafe` block must be explicitly `#[allow]`-ed and carry
//! a `// SAFETY:` comment explaining why it is sound.
//!
//! MFT *record parsing* is delegated to the safe [`ntfs`](https://crates.io/crates/ntfs)
//! crate, so our own `unsafe` surface is confined to volume-handle acquisition
//! and sector reads (wired up in Phase 1).
//!
//! On non-Windows platforms this crate compiles to a stub that reports
//! [`purify_core::Error::Unsupported`], allowing the CLI to fall back to a
//! portable directory walker.

use purify_core::{Result, Scanner};

/// High-performance scanner backed by direct NTFS MFT reads (Windows only).
///
/// Phase 0 provides only the type and its platform-gated availability check;
/// the actual MFT traversal lands in Phase 1.
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
    /// Returns `true` only on Windows. Callers should consult this and fall
    /// back to a portable [`Scanner`] when it returns `false`.
    #[must_use]
    pub fn is_available() -> bool {
        cfg!(windows)
    }
}

impl Scanner for MftScanner {
    fn scan(
        &self,
        _root: &std::path::Path,
        _sink: &mut dyn FnMut(purify_core::FileEntry),
    ) -> Result<()> {
        Err(purify_core::Error::Unsupported(
            "direct MFT scanning is not yet implemented (arrives in Phase 1)".to_string(),
        ))
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
    fn phase0_scan_is_unsupported() {
        let scanner = MftScanner::new();
        let mut count = 0;
        let result = scanner.scan(std::path::Path::new("."), &mut |_| count += 1);
        assert!(matches!(result, Err(purify_core::Error::Unsupported(_))));
        assert_eq!(count, 0);
    }
}
