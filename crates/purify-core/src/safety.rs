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

/// Absolute Windows path prefixes that must never be quarantined or moved.
///
/// These are the roots of the operating system, installed programs, and boot
/// artifacts. Touching anything under them risks an unbootable machine.
const PROTECTED_PREFIXES: &[&str] = &[
    r"c:\windows",
    r"c:\program files",
    r"c:\program files (x86)",
    r"c:\programdata\microsoft\windows",
    r"c:\$recycle.bin\s-1-5-18", // system SID recycle bin
    r"c:\system volume information",
    r"c:\perflogs",
    r"c:\boot",
    r"c:\recovery",
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

    PROTECTED_PREFIXES
        .iter()
        .any(|prefix| normalized == *prefix || normalized.starts_with(&format!("{prefix}\\")))
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
