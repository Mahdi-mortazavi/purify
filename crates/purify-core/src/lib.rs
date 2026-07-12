//! Core engine for **purify** — the pure-Rust, platform-agnostic heart of the
//! tool.
//!
//! This crate contains **no** `unsafe` and **no** OS-specific raw-device access.
//! Raw NTFS/volume access lives behind the [`Scanner`] trait and is implemented
//! in the `purify-ntfs` crate. Everything here can be unit tested against a
//! synthetic filesystem without touching a real disk.
//!
//! # Modules
//! - [`error`] — typed error and result.
//! - [`model`] — the [`FileEntry`] data model.
//! - [`scan`] — the [`Scanner`] trait and the portable [`WalkScanner`].
//! - [`usage`] — disk-usage aggregation into a [`usage::UsageReport`].
//! - [`safety`] — protected-path guards shared across the engine.
//!
//! Later phases add `rules`, `suggest`, `quarantine`, `organize`, and
//! `guardian` modules.

// The workspace forbids `unwrap`/`expect`/`panic` on production paths. Test
// code legitimately uses them for assertions, so relax the lints under `test`.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod error;
pub mod model;
pub mod safety;
pub mod scan;
pub mod usage;

pub use error::{Error, Result};
pub use model::FileEntry;
pub use scan::{Scanner, WalkScanner};
pub use usage::{Consumer, UsageCollector, UsageReport};

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
