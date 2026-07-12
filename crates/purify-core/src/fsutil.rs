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
/// volumes (EXDEV on Unix, ERROR_NOT_SAME_DEVICE on Windows).
fn is_cross_device(e: &std::io::Error) -> bool {
    matches!(e.raw_os_error(), Some(18) | Some(17))
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
