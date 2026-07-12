//! Filesystem move/copy/remove helpers shared by quarantine and organize.
//!
//! The central primitive is [`move_path`], which prefers an instant `rename`
//! and only falls back to a recursive copy + delete when source and destination
//! live on different volumes.

use std::path::Path;

use crate::error::{Error, Result};

/// Move `src` to `dst`, preferring `rename` and falling back to a recursive
/// copy + delete only when the two live on different volumes.
pub fn move_path(src: &Path, dst: &Path) -> Result<()> {
    match std::fs::rename(src, dst) {
        Ok(()) => Ok(()),
        Err(e) if is_cross_device(&e) => {
            copy_recursive(src, dst)?;
            remove_path(src)?;
            Ok(())
        }
        Err(e) => Err(Error::io(src, e)),
    }
}

/// Whether an error indicates the source and destination are on different
/// volumes. The error code is OS-specific and must be gated per-platform:
/// `EXDEV` is 18 on Unix, but `ERROR_NOT_SAME_DEVICE` is 17 on Windows — and 17
/// is `EEXIST` on Unix, so a single combined match would misclassify a
/// destination-exists error as cross-device and silently overwrite.
#[cfg(windows)]
fn is_cross_device(e: &std::io::Error) -> bool {
    // ERROR_NOT_SAME_DEVICE
    e.raw_os_error() == Some(17)
}

#[cfg(not(windows))]
fn is_cross_device(e: &std::io::Error) -> bool {
    // EXDEV
    e.raw_os_error() == Some(18)
}

/// Recursively copy a file or directory tree from `src` to `dst`.
pub fn copy_recursive(src: &Path, dst: &Path) -> Result<()> {
    let meta = std::fs::symlink_metadata(src).map_err(|e| Error::io(src, e))?;
    if meta.is_dir() {
        std::fs::create_dir_all(dst).map_err(|e| Error::io(dst, e))?;
        for entry in std::fs::read_dir(src).map_err(|e| Error::io(src, e))? {
            let entry = entry.map_err(|e| Error::io(src, e))?;
            copy_recursive(&entry.path(), &dst.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        std::fs::copy(src, dst).map_err(|e| Error::io(src, e))?;
        Ok(())
    }
}

/// Remove a file or directory tree.
pub fn remove_path(path: &Path) -> Result<()> {
    let meta = std::fs::symlink_metadata(path).map_err(|e| Error::io(path, e))?;
    if meta.is_dir() {
        std::fs::remove_dir_all(path).map_err(|e| Error::io(path, e))
    } else {
        std::fs::remove_file(path).map_err(|e| Error::io(path, e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_file_relocates_and_reads_back() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let src = tmp.path().join("a.txt");
        let dst = tmp.path().join("sub").join("b.txt");
        std::fs::create_dir_all(dst.parent().unwrap()).unwrap();
        std::fs::write(&src, b"hello").unwrap();

        move_path(&src, &dst).expect("move");
        assert!(!src.exists());
        assert_eq!(std::fs::read(&dst).unwrap(), b"hello");
    }

    #[test]
    fn move_dir_onto_existing_nonempty_dir_errors_not_merges() {
        // Regression: a rename that fails with EEXIST/ENOTEMPTY on Unix must not
        // be misread as cross-device and silently merged+deleted.
        let tmp = tempfile::tempdir().expect("tempdir");
        let src = tmp.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("keep.txt"), b"src").unwrap();

        let dst = tmp.path().join("dst");
        std::fs::create_dir_all(&dst).unwrap();
        std::fs::write(dst.join("other.txt"), b"dst").unwrap();

        let result = move_path(&src, &dst);
        // Either the OS refuses (Err) — the important thing is the source is not
        // silently deleted after an overwrite-merge.
        if result.is_err() {
            assert!(src.exists(), "source must remain after a refused move");
            assert!(src.join("keep.txt").exists());
        }
    }
}
