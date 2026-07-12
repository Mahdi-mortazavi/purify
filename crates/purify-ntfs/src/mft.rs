//! NTFS Master File Table traversal over any `Read + Seek` source.
//!
//! This is the heart of the fast scanner. It reads NTFS directory indexes
//! directly from the volume via the safe [`ntfs`] crate — no per-file `stat`
//! syscalls — and streams every discovered entry to a callback.
//!
//! Because it is generic over `Read + Seek`, it compiles and is unit-tested on
//! every platform against a real NTFS filesystem image (see the crate's
//! integration tests). Only the acquisition of a live volume handle is
//! Windows-specific.

use std::collections::HashSet;
use std::io::{Read, Seek};
use std::path::PathBuf;

use ntfs::structured_values::{NtfsFileName, NtfsFileNamespace};
use ntfs::Ntfs;
use purify_core::{Error, FileEntry, Result};

/// The NTFS root directory always lives at Master File Table record 5.
const ROOT_RECORD: u64 = 5;

/// MFT records below this number are reserved system metafiles ($MFT, $Bitmap,
/// $LogFile, …). They appear in the root index but are not user-visible files,
/// so we skip them.
const FIRST_USER_RECORD: u64 = 24;

/// Guards against pathological or malicious directory nesting.
const MAX_DEPTH: usize = 512;

fn ntfs_err(e: ntfs::NtfsError) -> Error {
    Error::Ntfs(e.to_string())
}

/// Traverse the NTFS filesystem readable from `fs`, invoking `sink` for every
/// file and directory discovered.
///
/// Directory recursion is done with an explicit stack (not native recursion) to
/// keep memory bounded and avoid deep call stacks on real drives.
pub fn scan_reader<T, F>(fs: &mut T, mut sink: F) -> Result<()>
where
    T: Read + Seek,
    F: FnMut(FileEntry),
{
    let mut ntfs = Ntfs::new(fs).map_err(ntfs_err)?;
    ntfs.read_upcase_table(fs).map_err(ntfs_err)?;

    // Stack of (mft record number, absolute path) directories still to visit.
    let mut stack: Vec<(u64, PathBuf, usize)> = vec![(ROOT_RECORD, PathBuf::from("\\"), 0)];
    let mut visited: HashSet<u64> = HashSet::new();

    while let Some((record, dir_path, depth)) = stack.pop() {
        if depth > MAX_DEPTH || !visited.insert(record) {
            continue;
        }

        let dir_file = match ntfs.file(fs, record) {
            Ok(f) => f,
            Err(e) => {
                tracing::warn!(record, error = %e, "skipping unreadable MFT record");
                continue;
            }
        };
        if !dir_file.is_directory() {
            continue;
        }

        let index = match dir_file.directory_index(fs) {
            Ok(i) => i,
            Err(e) => {
                tracing::warn!(record, error = %e, "skipping directory with unreadable index");
                continue;
            }
        };

        // Collect children first, then release the index/file borrows before
        // recursing into subdirectories.
        let mut children: Vec<(u64, PathBuf, bool, u64)> = Vec::new();
        let mut iter = index.entries();
        while let Some(entry) = iter.next(fs) {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    tracing::warn!(record, error = %e, "skipping unreadable index entry");
                    continue;
                }
            };

            let key: NtfsFileName = match entry.key() {
                Some(Ok(k)) => k,
                _ => continue,
            };

            // Each NTFS file can have both a Win32 and a DOS 8.3 name; skip the
            // DOS alias so every file is reported exactly once.
            if key.namespace() == NtfsFileNamespace::Dos {
                continue;
            }

            let child_record = entry.file_reference().file_record_number();
            if child_record < FIRST_USER_RECORD {
                continue; // reserved system metafile
            }

            let name = key.name().to_string_lossy();
            let child_path = dir_path.join(&name);
            children.push((
                child_record,
                child_path,
                key.is_directory(),
                key.data_size(),
            ));
        }
        drop(iter);
        drop(index);
        drop(dir_file);

        for (child_record, child_path, is_dir, size) in children {
            if is_dir {
                sink(FileEntry::dir(child_path.clone()));
                stack.push((child_record, child_path, depth + 1));
            } else {
                sink(FileEntry::file(child_path, size));
            }
        }
    }

    Ok(())
}
