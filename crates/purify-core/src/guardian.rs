//! **Guardian** — disk-space monitoring logic.
//!
//! Clutter is a flow, not an event, so a one-time cleanup is not enough. The
//! Guardian watches free space and warns *before* the drive fills up, nudging
//! the user to reclaim space while it is still easy.
//!
//! This module is pure logic over space numbers so it is fully testable; the
//! CLI (or a future background service) supplies live [`SpaceInfo`] from the OS
//! and renders/notifies on the resulting [`GuardianReport`].

use serde::Serialize;

/// A snapshot of a volume's capacity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SpaceInfo {
    /// Total capacity in bytes.
    pub total: u64,
    /// Available (free) bytes.
    pub available: u64,
}

impl SpaceInfo {
    /// Bytes currently used.
    #[must_use]
    pub fn used(self) -> u64 {
        self.total.saturating_sub(self.available)
    }

    /// Fraction of the volume that is used, in `0.0..=1.0`.
    #[must_use]
    pub fn used_fraction(self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        self.used() as f64 / self.total as f64
    }
}

/// How urgently the user should act.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Pressure {
    /// Plenty of free space.
    Ok,
    /// Getting full — worth reclaiming space soon.
    Warning,
    /// Nearly full — act now.
    Critical,
}

/// The thresholds (as used-fraction) at which pressure escalates.
#[derive(Debug, Clone, Copy)]
pub struct Thresholds {
    /// Used fraction at which to warn (default 0.85).
    pub warn: f64,
    /// Used fraction at which to flag critical (default 0.95).
    pub critical: f64,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            warn: 0.85,
            critical: 0.95,
        }
    }
}

/// The result of evaluating disk pressure.
#[derive(Debug, Clone, Serialize)]
pub struct GuardianReport {
    /// The space snapshot.
    pub space: SpaceInfo,
    /// Used fraction (0.0..=1.0).
    pub used_fraction: f64,
    /// The computed pressure level.
    pub pressure: Pressure,
    /// A human-readable recommendation.
    pub recommendation: String,
}

/// Evaluate disk pressure against thresholds.
#[must_use]
pub fn evaluate(space: SpaceInfo, thresholds: Thresholds) -> GuardianReport {
    let used = space.used_fraction();
    let pressure = if used >= thresholds.critical {
        Pressure::Critical
    } else if used >= thresholds.warn {
        Pressure::Warning
    } else {
        Pressure::Ok
    };
    let recommendation = match pressure {
        Pressure::Ok => "Disk space is healthy.".to_string(),
        Pressure::Warning => {
            "Disk is getting full. Run `purify analyze` to find reclaimable space.".to_string()
        }
        Pressure::Critical => {
            "Disk is nearly full. Run `purify clean --apply` to reclaim space now.".to_string()
        }
    };
    GuardianReport {
        space,
        used_fraction: used,
        pressure,
        recommendation,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(total: u64, available: u64) -> SpaceInfo {
        SpaceInfo { total, available }
    }

    #[test]
    fn healthy_disk_is_ok() {
        let r = evaluate(info(100, 50), Thresholds::default());
        assert_eq!(r.pressure, Pressure::Ok);
        assert!((r.used_fraction - 0.5).abs() < 1e-9);
    }

    #[test]
    fn warning_and_critical_thresholds() {
        let t = Thresholds::default();
        assert_eq!(evaluate(info(100, 14), t).pressure, Pressure::Warning); // 86% used
        assert_eq!(evaluate(info(100, 4), t).pressure, Pressure::Critical); // 96% used
    }

    #[test]
    fn zero_capacity_does_not_divide_by_zero() {
        let r = evaluate(info(0, 0), Thresholds::default());
        assert_eq!(r.pressure, Pressure::Ok);
        assert_eq!(r.used_fraction, 0.0);
    }
}
