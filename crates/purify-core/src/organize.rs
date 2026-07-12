//! File **organizer** — rule-driven tidying with preview and full undo.
//!
//! Where quarantine fights the *accumulation* of junk, the organizer fights the
//! *disorder* of real files: loose downloads, screenshots, documents. Rules map
//! files (by name pattern, location, and age) to a destination folder. Every
//! run produces a preview first, and applying a plan yields an undo log so the
//! move can be reversed exactly.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::fsutil::move_path;
use crate::model::FileEntry;
use crate::rules::{last_component, normalize, wildcard_match};
use crate::safety;

/// A single organization rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrganizeRule {
    /// Stable identifier.
    pub id: String,
    /// Human-readable description.
    pub description: String,
    /// File-name wildcard patterns (e.g. `*.pdf`).
    pub patterns: Vec<String>,
    /// Optional path substring the file must be located within.
    #[serde(default)]
    pub within: Option<String>,
    /// Optional minimum age in days.
    #[serde(default)]
    pub min_age_days: Option<u32>,
    /// Destination subdirectory (relative to the organize base).
    pub dest_subdir: String,
}

impl OrganizeRule {
    fn matches(&self, entry: &FileEntry, now_unix: i64) -> bool {
        if entry.is_dir {
            return false;
        }
        if let Some(w) = &self.within {
            if !normalize(&entry.path.to_string_lossy()).contains(&normalize(w)) {
                return false;
            }
        }
        if let Some(days) = self.min_age_days {
            let Some(modified) = entry.modified else {
                return false;
            };
            if now_unix.saturating_sub(modified) < i64::from(days) * 86_400 {
                return false;
            }
        }
        let name = last_component(&entry.path);
        self.patterns.iter().any(|p| wildcard_match(&name, p))
    }
}

/// A planned move of one file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Move {
    /// Source path.
    pub from: PathBuf,
    /// Destination path.
    pub to: PathBuf,
    /// The rule that produced this move.
    pub rule_id: String,
    /// File size in bytes.
    pub size: u64,
}

/// A preview of all moves an organize run would perform.
#[derive(Debug, Clone, Default, Serialize)]
pub struct OrganizePlan {
    /// The planned moves.
    pub moves: Vec<Move>,
}

/// A record of moves actually performed, enabling exact undo.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OrganizeLog {
    /// Pairs of (original source, final destination).
    pub moves: Vec<(PathBuf, PathBuf)>,
}

/// The organizer: a set of rules applied to a scan.
#[derive(Debug, Clone)]
pub struct Organizer {
    rules: Vec<OrganizeRule>,
}

impl Organizer {
    /// Create an organizer from explicit rules.
    #[must_use]
    pub fn new(rules: Vec<OrganizeRule>) -> Self {
        Self { rules }
    }

    /// A sensible default rule set: file rovers into typed archive folders.
    #[must_use]
    pub fn with_defaults() -> Self {
        let rule =
            |id: &str, desc: &str, pats: &[&str], sub: &str, age: Option<u32>| OrganizeRule {
                id: id.to_string(),
                description: desc.to_string(),
                patterns: pats.iter().map(|s| s.to_string()).collect(),
                within: None,
                min_age_days: age,
                dest_subdir: sub.to_string(),
            };
        Self::new(vec![
            rule(
                "documents",
                "Documents",
                &["*.pdf", "*.doc", "*.docx", "*.txt", "*.odt"],
                "Documents",
                Some(30),
            ),
            rule(
                "spreadsheets",
                "Spreadsheets",
                &["*.xls", "*.xlsx", "*.csv", "*.ods"],
                "Documents/Spreadsheets",
                Some(30),
            ),
            rule(
                "images",
                "Images",
                &["*.jpg", "*.jpeg", "*.png", "*.gif", "*.webp"],
                "Pictures",
                Some(30),
            ),
            rule(
                "archives",
                "Archives",
                &["*.zip", "*.7z", "*.rar", "*.tar", "*.gz"],
                "Archives",
                Some(30),
            ),
            rule(
                "installers",
                "Installers",
                &["*.msi", "*.exe"],
                "Installers",
                Some(60),
            ),
            rule(
                "audio",
                "Audio",
                &["*.mp3", "*.flac", "*.wav", "*.m4a"],
                "Music",
                Some(30),
            ),
            rule(
                "video",
                "Video",
                &["*.mp4", "*.mkv", "*.mov", "*.avi"],
                "Videos",
                Some(30),
            ),
        ])
    }

    /// The number of rules.
    #[must_use]
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// Build a plan for organizing `entries` into folders under `dest_base`.
    ///
    /// Files already under `dest_base` are left alone (so re-running is
    /// idempotent), protected files are skipped, and destination collisions are
    /// resolved by appending ` (n)` before the extension.
    #[must_use]
    pub fn plan(&self, entries: &[FileEntry], dest_base: &Path, now_unix: i64) -> OrganizePlan {
        let mut moves = Vec::new();
        let mut claimed: Vec<PathBuf> = Vec::new();

        for entry in entries {
            if entry.is_dir || safety::is_protected(&entry.path) {
                continue;
            }
            // Do not re-organize files already inside the destination tree.
            if entry.path.starts_with(dest_base) {
                continue;
            }
            let Some(rule) = self.rules.iter().find(|r| r.matches(entry, now_unix)) else {
                continue;
            };
            let name = last_component(&entry.path);
            let dest_dir = dest_base.join(&rule.dest_subdir);
            let mut dest = dest_dir.join(&name);
            let mut n = 1;
            while dest == entry.path || claimed.contains(&dest) || dest.exists() {
                dest = dest_dir.join(disambiguate(&name, n));
                n += 1;
            }
            if dest == entry.path {
                continue;
            }
            claimed.push(dest.clone());
            moves.push(Move {
                from: entry.path.clone(),
                to: dest,
                rule_id: rule.id.clone(),
                size: entry.size,
            });
        }
        OrganizePlan { moves }
    }

    /// Execute a plan, moving each file and returning an undo log.
    ///
    /// A failure on one move is logged and skipped so a single problem file does
    /// not abort the whole run; already-moved files remain in the returned log
    /// and can be undone.
    pub fn apply(plan: &OrganizePlan) -> Result<OrganizeLog> {
        let mut log = OrganizeLog::default();
        for mv in &plan.moves {
            if let Some(parent) = mv.to.parent() {
                std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
            }
            match move_path(&mv.from, &mv.to) {
                Ok(()) => log.moves.push((mv.from.clone(), mv.to.clone())),
                Err(err) => {
                    tracing::warn!(from = %mv.from.display(), %err, "skipping move");
                }
            }
        }
        Ok(log)
    }

    /// Reverse the moves in a log, restoring files to their original locations.
    pub fn undo(log: &OrganizeLog) -> Result<()> {
        // Reverse order so nested creations unwind cleanly.
        for (from, to) in log.moves.iter().rev() {
            if !to.exists() {
                continue;
            }
            if let Some(parent) = from.parent() {
                std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
            }
            move_path(to, from)?;
        }
        Ok(())
    }
}

/// Insert ` (n)` before the extension of a file name.
fn disambiguate(name: &str, n: u32) -> String {
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => format!("{stem} ({n}).{ext}"),
        _ => format!("{name} ({n})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn old_file(path: &str, size: u64) -> FileEntry {
        FileEntry::file(path, size).with_modified(Some(0))
    }

    #[test]
    fn plans_moves_by_type_and_age() {
        let org = Organizer::with_defaults();
        let now = 1000 * 86_400;
        let entries = vec![
            old_file("/dl/report.pdf", 100),
            old_file("/dl/photo.png", 200),
            old_file("/dl/notes.rs", 50), // no rule
        ];
        let plan = org.plan(&entries, Path::new("/dl/Organized"), now);
        assert_eq!(plan.moves.len(), 2);
        let pdf = plan
            .moves
            .iter()
            .find(|m| m.rule_id == "documents")
            .unwrap();
        assert_eq!(pdf.to, PathBuf::from("/dl/Organized/Documents/report.pdf"));
    }

    #[test]
    fn recent_files_are_left_alone() {
        let org = Organizer::with_defaults();
        let now = 1000 * 86_400;
        let recent = FileEntry::file("/dl/fresh.pdf", 100).with_modified(Some(now - 86_400));
        let plan = org.plan(&[recent], Path::new("/dl/Organized"), now);
        assert!(plan.moves.is_empty(), "younger than 30 days");
    }

    #[test]
    fn apply_and_undo_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let dl = tmp.path().join("Downloads");
        std::fs::create_dir_all(&dl).unwrap();
        let file = dl.join("old.pdf");
        std::fs::write(&file, b"hello").unwrap();

        let entries = vec![FileEntry::file(&file, 5).with_modified(Some(0))];
        let dest = dl.join("Organized");
        let org = Organizer::with_defaults();
        let plan = org.plan(&entries, &dest, 1000 * 86_400);
        assert_eq!(plan.moves.len(), 1);

        let log = Organizer::apply(&plan).unwrap();
        assert!(!file.exists(), "moved out of place");
        assert!(dest.join("Documents/old.pdf").exists(), "moved into place");

        Organizer::undo(&log).unwrap();
        assert!(file.exists(), "restored to original location");
        assert_eq!(std::fs::read(&file).unwrap(), b"hello");
    }

    #[test]
    fn protected_files_are_never_moved() {
        let org = Organizer::with_defaults();
        let entries = vec![old_file(r"C:\Windows\System32\report.pdf", 100)];
        let plan = org.plan(&entries, Path::new(r"C:\Organized"), 1000 * 86_400);
        assert!(plan.moves.is_empty());
    }
}
