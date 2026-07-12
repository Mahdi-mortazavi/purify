//! Disk-usage aggregation: turn a stream of [`FileEntry`] into a report of the
//! biggest space consumers directly under a scanned root.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::model::FileEntry;

/// One entry in a usage report: an immediate child of the scanned root, with
/// the total number of bytes accounted to it (recursively, for directories).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Consumer {
    /// Absolute path of the immediate child.
    pub path: PathBuf,
    /// Total bytes attributed to this child.
    pub size: u64,
    /// Whether the child is a directory.
    pub is_dir: bool,
}

/// A summary of what fills a scanned location.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UsageReport {
    /// The scanned root.
    pub root: PathBuf,
    /// Total bytes across all files under the root.
    pub total_bytes: u64,
    /// Number of files counted.
    pub file_count: u64,
    /// Largest immediate children, sorted by size descending.
    pub top: Vec<Consumer>,
}

/// Accumulates [`FileEntry`] records into a [`UsageReport`].
///
/// Rather than build a full in-memory tree (which is costly on drives with tens
/// of millions of files), we attribute every file's bytes to the immediate
/// child of the root that contains it. This answers the question users actually
/// ask — "which folder is eating my disk?" — in O(files) time and O(children)
/// memory.
#[derive(Debug)]
pub struct UsageCollector {
    root: PathBuf,
    children: HashMap<PathBuf, (u64, bool)>,
    total_bytes: u64,
    file_count: u64,
}

impl UsageCollector {
    /// Create a collector for a given scan root.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            children: HashMap::new(),
            total_bytes: 0,
            file_count: 0,
        }
    }

    /// Record a scanned entry. Directories are ignored (their size is derived
    /// from the files they contain); only files contribute bytes.
    pub fn record(&mut self, entry: &FileEntry) {
        if entry.is_dir {
            return;
        }
        self.total_bytes = self.total_bytes.saturating_add(entry.size);
        self.file_count += 1;

        if let Some((child_path, is_dir)) = self.immediate_child(&entry.path) {
            let slot = self.children.entry(child_path).or_insert((0, is_dir));
            slot.0 = slot.0.saturating_add(entry.size);
        }
    }

    /// Determine which immediate child of the root a path belongs to.
    ///
    /// Returns the child path and whether the child is a directory (i.e. the
    /// file lives deeper than one level below the root).
    fn immediate_child(&self, path: &Path) -> Option<(PathBuf, bool)> {
        let rel = path.strip_prefix(&self.root).ok()?;
        let mut components = rel.components();
        let first = components.next()?;
        let child = self.root.join(first.as_os_str());
        // If there are more components after the first, the child is a directory
        // containing this file; otherwise the child *is* the file itself.
        let is_dir = components.next().is_some();
        Some((child, is_dir))
    }

    /// Finalize into a report keeping the `top` largest children.
    #[must_use]
    pub fn into_report(self, top: usize) -> UsageReport {
        let mut consumers: Vec<Consumer> = self
            .children
            .into_iter()
            .map(|(path, (size, is_dir))| Consumer { path, size, is_dir })
            .collect();
        // Sort by size descending, then path for deterministic ordering.
        consumers.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.path.cmp(&b.path)));
        consumers.truncate(top);

        UsageReport {
            root: self.root,
            total_bytes: self.total_bytes,
            file_count: self.file_count,
            top: consumers,
        }
    }
}

/// An index of every directory's recursive byte size, built from a scan.
///
/// Used by the rule engine to size a matched directory (e.g. a `node_modules`
/// folder) without re-walking it. Each file contributes its size to every one
/// of its ancestor directories exactly once, so a lookup returns the total
/// bytes beneath a directory.
#[derive(Debug, Default)]
pub struct DirSizeIndex {
    sizes: HashMap<PathBuf, u64>,
}

impl DirSizeIndex {
    /// Build the index from a full list of scanned entries.
    ///
    /// Parallelized with `rayon`: each worker folds a slice of files into its own
    /// map, then the per-worker maps are reduced together. Each file attributes
    /// its bytes to every ancestor directory exactly once.
    #[must_use]
    pub fn build(entries: &[FileEntry]) -> Self {
        use rayon::prelude::*;

        let sizes = entries
            .par_iter()
            .filter(|e| !e.is_dir && e.size > 0)
            .fold(HashMap::<PathBuf, u64>::new, |mut acc, entry| {
                for ancestor in entry.path.ancestors().skip(1) {
                    if ancestor.as_os_str().is_empty() {
                        continue;
                    }
                    *acc.entry(ancestor.to_path_buf()).or_insert(0) += entry.size;
                }
                acc
            })
            .reduce(HashMap::new, |mut a, b| {
                for (k, v) in b {
                    *a.entry(k).or_insert(0) += v;
                }
                a
            });
        Self { sizes }
    }

    /// Build an index that sizes **only** the given directories.
    ///
    /// This is much cheaper than [`DirSizeIndex::build`] when only a handful of
    /// directories are of interest (the usual case after pruning): each file
    /// walks its ancestors and does an allocation-free set lookup, allocating a
    /// key only for the (few) directories actually being sized. A pruned set of
    /// target dirs is non-nested, so each file contributes to at most one.
    #[must_use]
    pub fn build_for(entries: &[FileEntry], dirs: &[PathBuf]) -> Self {
        use rayon::prelude::*;
        use std::collections::HashSet;

        if dirs.is_empty() {
            return Self::default();
        }
        let targets: HashSet<&Path> = dirs.iter().map(PathBuf::as_path).collect();

        let sizes = entries
            .par_iter()
            .filter(|e| !e.is_dir && e.size > 0)
            .fold(HashMap::<PathBuf, u64>::new, |mut acc, entry| {
                for ancestor in entry.path.ancestors().skip(1) {
                    if targets.contains(ancestor) {
                        *acc.entry(ancestor.to_path_buf()).or_insert(0) += entry.size;
                        break; // pruned targets are non-nested: at most one match
                    }
                }
                acc
            })
            .reduce(HashMap::new, |mut a, b| {
                for (k, v) in b {
                    *a.entry(k).or_insert(0) += v;
                }
                a
            });
        Self { sizes }
    }

    /// Total bytes stored beneath `dir` (0 if unknown).
    #[must_use]
    pub fn size_of(&self, dir: &Path) -> u64 {
        self.sizes.get(dir).copied().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dir_size_index_sums_descendants() {
        let entries = vec![
            FileEntry::file("/root/a/b/one.bin", 1000),
            FileEntry::file("/root/a/b/two.bin", 200),
            FileEntry::file("/root/a/three.bin", 50),
            FileEntry::dir("/root/a"),
        ];
        let idx = DirSizeIndex::build(&entries);
        assert_eq!(idx.size_of(Path::new("/root/a/b")), 1200);
        assert_eq!(idx.size_of(Path::new("/root/a")), 1250);
        assert_eq!(idx.size_of(Path::new("/root")), 1250);
        assert_eq!(idx.size_of(Path::new("/nonexistent")), 0);
    }

    #[test]
    fn attributes_bytes_to_immediate_children() {
        let root = Path::new("/root");
        let mut c = UsageCollector::new(root);
        c.record(&FileEntry::file("/root/big/one.bin", 1000));
        c.record(&FileEntry::file("/root/big/two.bin", 500));
        c.record(&FileEntry::file("/root/small.txt", 10));
        c.record(&FileEntry::dir("/root/big")); // ignored

        let report = c.into_report(10);
        assert_eq!(report.total_bytes, 1510);
        assert_eq!(report.file_count, 3);
        assert_eq!(report.top.len(), 2);

        // Largest child is the "big" directory at 1500 bytes.
        assert_eq!(report.top[0].path, PathBuf::from("/root/big"));
        assert_eq!(report.top[0].size, 1500);
        assert!(report.top[0].is_dir);

        // Second is the loose file.
        assert_eq!(report.top[1].path, PathBuf::from("/root/small.txt"));
        assert_eq!(report.top[1].size, 10);
        assert!(!report.top[1].is_dir);
    }

    #[test]
    fn top_n_truncates() {
        let root = Path::new("/r");
        let mut c = UsageCollector::new(root);
        for i in 0..5 {
            c.record(&FileEntry::file(
                format!("/r/d{i}/f.bin"),
                (i as u64 + 1) * 100,
            ));
        }
        let report = c.into_report(2);
        assert_eq!(report.top.len(), 2);
        assert_eq!(report.top[0].size, 500);
        assert_eq!(report.top[1].size, 400);
    }
}
