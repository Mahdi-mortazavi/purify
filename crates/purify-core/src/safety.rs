//! Safety guards shared across the engine.
//!
//! purify's first principle is "never touch critical system files." This module
//! centralizes the check so that every destructive-ish operation (quarantine,
//! organize) can consult one authoritative allowlist/denylist instead of
//! re-implementing ad-hoc string checks.
//!
//! The logic is deliberately conservative: when in doubt, a path is treated as
//! protected. It is also case-insensitive, because Windows paths are.

use std::path::Path;

/// Drive-relative Windows path prefixes that must never be quarantined or moved.
///
/// These are the roots of the operating system, installed programs, and boot
/// artifacts. Touching anything under them risks an unbootable machine. They are
/// matched against the path with its `<letter>:` drive stripped, so they protect
/// these locations on **any** drive (Windows can be installed on D:, programs
/// relocated, etc.) — not just C:.
const PROTECTED_PREFIXES: &[&str] = &[
    r"\windows",
    r"\program files",
    r"\program files (x86)",
    r"\programdata\microsoft\windows",
    r"\$recycle.bin\s-1-5-18", // system SID recycle bin
    r"\system volume information",
    r"\perflogs",
    r"\boot",
    r"\recovery",
];

/// Drive-relative subtrees that are **safe to reclaim even though they sit
/// under an otherwise-protected prefix**. These are OS-managed caches that
/// Windows regenerates and that Windows' own Disk Cleanup removes. Matching one
/// of these overrides the broader protected-prefix rule (but never the
/// pseudo-file-name rule below).
const SAFE_SUBTREES: &[&str] = &[
    r"\windows\temp",
    r"\windows\prefetch",
    r"\windows\logs",
    r"\windows\softwaredistribution\download",
    r"\windows\softwaredistribution\deliveryoptimization",
    r"\programdata\microsoft\windows\wer",
];

/// File names that are protected regardless of location — the big Windows
/// pseudo-files. We never move or quarantine these; freeing them is a
/// deliberate, separately-gated operation (e.g. via `powercfg`), not a file op.
const PROTECTED_FILE_NAMES: &[&str] = &[
    "pagefile.sys",
    "hiberfil.sys",
    "swapfile.sys",
    "bootmgr",
    "ntldr",
    "bootnxt",
];

/// Normalize a path to a lowercase string with backslashes, for comparison.
fn normalize(path: &Path) -> String {
    path.to_string_lossy()
        .to_ascii_lowercase()
        .replace('/', "\\")
}

/// Whether `path` is protected and must never be quarantined, moved, or deleted.
///
/// This is intentionally conservative and returns `true` for:
/// - anything under a known OS/program prefix,
/// - any of the protected pseudo-files by name,
/// - a drive root itself (e.g. `C:\`).
#[must_use]
pub fn is_protected(path: &Path) -> bool {
    let normalized = normalize(path);

    // Protected pseudo-files by name, anywhere. We derive the final component
    // from the normalized (backslash) string rather than `Path::file_name()`,
    // because on non-Windows hosts (CI, tests) `\` is not a path separator and
    // `file_name()` would return the whole `C:\pagefile.sys` string.
    let last_component = normalized.trim_end_matches('\\').rsplit('\\').next();
    if let Some(name) = last_component {
        if PROTECTED_FILE_NAMES.contains(&name) {
            return true;
        }
    }

    // A bare drive root like `c:\` or `c:`.
    if is_drive_root(&normalized) {
        return true;
    }

    // Strip a leading `<letter>:` so the drive-relative prefixes match on any
    // drive (Windows/Program Files can live on D:, E:, ...). If there is no
    // drive letter, match against the whole normalized path.
    let drive_relative = strip_drive(&normalized);

    // Well-known OS caches under a protected prefix are explicitly reclaimable.
    if SAFE_SUBTREES
        .iter()
        .any(|safe| under_prefix(drive_relative, safe))
    {
        return false;
    }

    PROTECTED_PREFIXES
        .iter()
        .any(|prefix| under_prefix(drive_relative, prefix))
}

/// Whether `path` equals `prefix` or is nested beneath it, comparing whole
/// components (so `\windows` does not match `\windows.old`).
fn under_prefix(path: &str, prefix: &str) -> bool {
    path == prefix || path.starts_with(&format!("{prefix}\\"))
}

/// Remove a leading `<letter>:` from a normalized path, returning the remainder
/// (which keeps its leading `\`). Paths without a drive letter are returned
/// unchanged.
fn strip_drive(normalized: &str) -> &str {
    let bytes = normalized.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
        &normalized[2..]
    } else {
        normalized
    }
}

/// Whether the normalized path is a drive root such as `c:\` or `c:`.
fn is_drive_root(normalized: &str) -> bool {
    let trimmed = normalized.trim_end_matches('\\');
    // e.g. "c:" — two chars, second is ':'.
    trimmed.len() == 2 && trimmed.as_bytes()[1] == b':'
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn windows_system_dirs_are_protected() {
        assert!(is_protected(&PathBuf::from(
            r"C:\Windows\System32\kernel32.dll"
        )));
        assert!(is_protected(&PathBuf::from(
            r"C:\Program Files\App\bin.exe"
        )));
        assert!(is_protected(&PathBuf::from(
            r"c:\program files (x86)\x\y.dll"
        )));
    }

    #[test]
    fn system_dirs_are_protected_on_any_drive() {
        // Windows can live on D:, programs can be relocated to E:, etc.
        assert!(is_protected(&PathBuf::from(
            r"D:\Windows\System32\kernel32.dll"
        )));
        assert!(is_protected(&PathBuf::from(r"E:\Program Files\App\x.dll")));
        assert!(is_protected(&PathBuf::from(r"F:\Boot\bootmgr.efi")));
    }

    #[test]
    fn case_and_slash_insensitive() {
        assert!(is_protected(&PathBuf::from(r"C:/WINDOWS/system32")));
    }

    #[test]
    fn protected_pseudo_files_by_name() {
        assert!(is_protected(&PathBuf::from(r"C:\pagefile.sys")));
        assert!(is_protected(&PathBuf::from(r"D:\hiberfil.sys")));
    }

    #[test]
    fn drive_root_is_protected() {
        assert!(is_protected(&PathBuf::from(r"C:\")));
        assert!(is_protected(&PathBuf::from("C:")));
    }

    #[test]
    fn safe_os_caches_under_windows_are_reclaimable() {
        // These sit under \Windows (protected) but are explicitly safe.
        assert!(!is_protected(&PathBuf::from(r"C:\Windows\Temp\x.tmp")));
        assert!(!is_protected(&PathBuf::from(r"C:\Windows\Prefetch\APP.pf")));
        assert!(!is_protected(&PathBuf::from(
            r"C:\Windows\SoftwareDistribution\Download\abc\update.cab"
        )));
        assert!(!is_protected(&PathBuf::from(
            r"C:\ProgramData\Microsoft\Windows\WER\ReportQueue\x"
        )));
    }

    #[test]
    fn critical_windows_dirs_stay_protected_despite_carveouts() {
        assert!(is_protected(&PathBuf::from(
            r"C:\Windows\System32\kernel32.dll"
        )));
        assert!(is_protected(&PathBuf::from(r"C:\Windows\WinSxS\x")));
        assert!(!is_protected(&PathBuf::from(r"C:\Windows\Temp")));
        // pagefile stays protected even by name, anywhere.
        assert!(is_protected(&PathBuf::from(
            r"C:\Windows\Temp\pagefile.sys"
        )));
    }

    #[test]
    fn ordinary_user_files_are_not_protected() {
        assert!(!is_protected(&PathBuf::from(
            r"C:\Users\me\Downloads\setup.exe"
        )));
        assert!(!is_protected(&PathBuf::from(
            r"D:\projects\app\node_modules\x"
        )));
        // A prefix that only *starts* like a protected one but isn't a boundary.
        assert!(!is_protected(&PathBuf::from(r"C:\Windows Games\save.dat")));
    }
}
