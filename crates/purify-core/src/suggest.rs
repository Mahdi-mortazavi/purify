//! Turns a scan plus a [`SignatureSet`] into concrete, de-duplicated cleanup
//! [`Suggestion`]s — the "decide" half of the product thesis.

use std::path::PathBuf;

use serde::Serialize;

use crate::model::FileEntry;
use crate::rules::{Confidence, SignatureSet};
use crate::usage::DirSizeIndex;

/// A concrete recommendation to reclaim a specific path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Suggestion {
    /// The signature that produced this suggestion.
    pub signature_id: String,
    /// Human-readable title of the category.
    pub title: String,
    /// Category grouping.
    pub category: String,
    /// Confidence label (`safe` / `likely-safe` / `review-needed`).
    pub confidence: String,
    /// The path recommended for cleanup.
    pub path: PathBuf,
    /// Whether the path is a directory.
    pub is_dir: bool,
    /// Bytes that would be reclaimed.
    pub size: u64,
    /// Why this is safe/recommended (from the signature description).
    pub reason: String,
}

/// The result of analyzing a scan against a signature set.
#[derive(Debug, Clone, Default, Serialize)]
pub struct AnalysisReport {
    /// All suggestions, sorted by size descending.
    pub suggestions: Vec<Suggestion>,
    /// Total reclaimable bytes across all suggestions.
    pub total_reclaimable: u64,
}

impl AnalysisReport {
    /// Total reclaimable bytes for suggestions at or below a confidence rank.
    #[must_use]
    pub fn reclaimable_up_to(&self, max: Confidence) -> u64 {
        self.suggestions
            .iter()
            .filter(|s| confidence_rank(&s.confidence) <= max.rank())
            .map(|s| s.size)
            .sum()
    }
}

fn confidence_rank(label: &str) -> u8 {
    match label {
        "safe" => 0,
        "likely-safe" => 1,
        _ => 2,
    }
}

/// Analyzes scanned entries against signatures to produce suggestions.
#[derive(Debug)]
pub struct Analyzer {
    signatures: SignatureSet,
}

impl Analyzer {
    /// Create an analyzer from a signature set.
    #[must_use]
    pub fn new(signatures: SignatureSet) -> Self {
        Self { signatures }
    }

    /// Analyze a full list of scanned entries.
    ///
    /// `now_unix` drives age-based rules. Suggestions are de-duplicated so that
    /// when a whole directory is recommended, individual files beneath it are
    /// not also listed.
    #[must_use]
    pub fn analyze(&self, entries: &[FileEntry], now_unix: i64) -> AnalysisReport {
        let dir_index = DirSizeIndex::build(entries);

        // First pass: find the first matching signature for each entry.
        struct Match<'a> {
            entry: &'a FileEntry,
            sig: &'a crate::rules::Signature,
        }
        let mut matches: Vec<Match> = Vec::new();
        for entry in entries {
            // Never suggest a protected path, no matter what a signature says.
            if crate::safety::is_protected(&entry.path) {
                continue;
            }
            if let Some(sig) = self
                .signatures
                .signatures
                .iter()
                .find(|s| s.rule.matches(entry, now_unix))
            {
                matches.push(Match { entry, sig });
            }
        }

        // Second pass: prune matches nested under an already-accepted directory,
        // so a cache directory is suggested once rather than per-file.
        matches.sort_by_key(|m| m.entry.path.as_os_str().len());
        let mut accepted_dirs: Vec<PathBuf> = Vec::new();
        let mut suggestions: Vec<Suggestion> = Vec::new();

        for m in matches {
            let path = &m.entry.path;
            // Matches are sorted shortest-path-first, so any accepted directory
            // that is a prefix of this path is a true ancestor already covering
            // it. `starts_with` compares whole components, so `\downloads` does
            // not spuriously prefix `\downloads-old`.
            if accepted_dirs.iter().any(|d| path.starts_with(d)) {
                continue;
            }

            let size = if m.entry.is_dir {
                dir_index.size_of(path)
            } else {
                m.entry.size
            };
            if size == 0 {
                continue; // nothing to reclaim
            }

            if m.entry.is_dir {
                accepted_dirs.push(path.clone());
            }

            suggestions.push(Suggestion {
                signature_id: m.sig.id.clone(),
                title: m.sig.title.clone(),
                category: m.sig.category.clone(),
                confidence: m.sig.confidence.label().to_string(),
                path: path.clone(),
                is_dir: m.entry.is_dir,
                size,
                reason: m.sig.description.clone(),
            });
        }

        suggestions.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.path.cmp(&b.path)));
        let total_reclaimable = suggestions.iter().map(|s| s.size).sum();

        AnalysisReport {
            suggestions,
            total_reclaimable,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{MatchRule, Signature};

    fn sig(id: &str, conf: Confidence, rule: MatchRule) -> Signature {
        Signature {
            id: id.to_string(),
            title: id.to_string(),
            category: "test".to_string(),
            confidence: conf,
            description: "because".to_string(),
            rule,
        }
    }

    #[test]
    fn suggests_directory_once_and_sizes_it() {
        let set = SignatureSet {
            signatures: vec![sig(
                "nm",
                Confidence::ReviewNeeded,
                MatchRule::DirName {
                    name: "node_modules".to_string(),
                },
            )],
        };
        // Forward-slash paths so std::path treats them as separators on the
        // (non-Windows) test host; on Windows real backslash paths behave the
        // same way.
        let entries = vec![
            FileEntry::dir("/proj/node_modules"),
            FileEntry::file("/proj/node_modules/a.js", 1000),
            FileEntry::file("/proj/node_modules/b.js", 500),
            FileEntry::file("/proj/src/main.rs", 42),
        ];
        let report = Analyzer::new(set).analyze(&entries, 0);
        assert_eq!(report.suggestions.len(), 1, "one directory suggestion");
        let s = &report.suggestions[0];
        assert_eq!(s.path, PathBuf::from("/proj/node_modules"));
        assert_eq!(s.size, 1500, "recursive directory size");
        assert_eq!(report.total_reclaimable, 1500);
    }

    #[test]
    fn protected_paths_are_never_suggested() {
        let set = SignatureSet {
            signatures: vec![sig(
                "temp",
                Confidence::Safe,
                MatchRule::PathContains {
                    needle: r"\windows\".to_string(),
                },
            )],
        };
        let entries = vec![FileEntry::file(r"C:\Windows\System32\x.dll", 9999)];
        let report = Analyzer::new(set).analyze(&entries, 0);
        assert!(report.suggestions.is_empty(), "protected path excluded");
    }

    #[test]
    fn reclaimable_up_to_filters_by_confidence() {
        let set = SignatureSet {
            signatures: vec![
                sig(
                    "safe-cache",
                    Confidence::Safe,
                    MatchRule::DirName {
                        name: "cache".to_string(),
                    },
                ),
                sig(
                    "risky",
                    Confidence::ReviewNeeded,
                    MatchRule::DirName {
                        name: "node_modules".to_string(),
                    },
                ),
            ],
        };
        let entries = vec![
            FileEntry::dir("/app/cache"),
            FileEntry::file("/app/cache/x", 100),
            FileEntry::dir("/app/node_modules"),
            FileEntry::file("/app/node_modules/y", 1000),
        ];
        let report = Analyzer::new(set).analyze(&entries, 0);
        assert_eq!(report.total_reclaimable, 1100);
        assert_eq!(report.reclaimable_up_to(Confidence::Safe), 100);
        assert_eq!(report.reclaimable_up_to(Confidence::ReviewNeeded), 1100);
    }
}
