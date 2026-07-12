//! Turns a scan plus a [`SignatureSet`] into concrete, de-duplicated cleanup
//! [`Suggestion`]s — the "decide" half of the product thesis.
//!
//! # Performance
//! Signatures are *compiled* once (needles and patterns pre-lowercased) when the
//! [`Analyzer`] is built, each scanned path is normalized exactly once, and the
//! per-entry matching runs in parallel across CPU cores via `rayon`. This turns
//! the analysis hot loop from O(files × signatures) string allocations into a
//! single normalization per file plus allocation-free comparisons.

use std::path::PathBuf;

use rayon::prelude::*;
use serde::Serialize;

use crate::model::FileEntry;
use crate::rules::{normalize, wildcard_match_lower, Confidence, MatchRule, SignatureSet};
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

/// A signature with its match inputs pre-lowercased for allocation-free matching.
#[derive(Debug)]
struct CompiledSig {
    id: String,
    title: String,
    category: String,
    confidence: Confidence,
    description: String,
    rule: CompiledRule,
}

#[derive(Debug)]
enum CompiledRule {
    DirName {
        name: String,
    },
    PathContains {
        needle: String,
    },
    Glob {
        patterns: Vec<String>,
        within: Option<String>,
        min_age_days: Option<u32>,
    },
}

impl CompiledRule {
    fn compile(rule: &MatchRule) -> Self {
        match rule {
            MatchRule::DirName { name } => CompiledRule::DirName {
                name: name.to_ascii_lowercase(),
            },
            MatchRule::PathContains { needle } => CompiledRule::PathContains {
                needle: normalize(needle),
            },
            MatchRule::Glob {
                patterns,
                within,
                min_age_days,
            } => CompiledRule::Glob {
                patterns: patterns.iter().map(|p| p.to_ascii_lowercase()).collect(),
                within: within.as_deref().map(normalize),
                min_age_days: *min_age_days,
            },
        }
    }

    /// Match against a pre-normalized (lowercased, backslash) path and its
    /// lowercased final component. No allocations.
    fn matches(
        &self,
        norm_path: &str,
        name_lower: &str,
        is_dir: bool,
        modified: Option<i64>,
        now: i64,
    ) -> bool {
        match self {
            CompiledRule::DirName { name } => is_dir && name_lower == name,
            CompiledRule::PathContains { needle } => norm_path.contains(needle.as_str()),
            CompiledRule::Glob {
                patterns,
                within,
                min_age_days,
            } => {
                if is_dir {
                    return false;
                }
                if let Some(w) = within {
                    if !norm_path.contains(w.as_str()) {
                        return false;
                    }
                }
                if let Some(days) = min_age_days {
                    let Some(m) = modified else {
                        return false;
                    };
                    if now.saturating_sub(m) < i64::from(*days) * 86_400 {
                        return false;
                    }
                }
                patterns.iter().any(|p| wildcard_match_lower(name_lower, p))
            }
        }
    }
}

/// Analyzes scanned entries against signatures to produce suggestions.
#[derive(Debug)]
pub struct Analyzer {
    signatures: Vec<CompiledSig>,
}

impl Analyzer {
    /// Create an analyzer from a signature set, compiling it for fast matching.
    #[must_use]
    pub fn new(signatures: SignatureSet) -> Self {
        let compiled = signatures
            .signatures
            .into_iter()
            .map(|s| CompiledSig {
                rule: CompiledRule::compile(&s.rule),
                id: s.id,
                title: s.title,
                category: s.category,
                confidence: s.confidence,
                description: s.description,
            })
            .collect();
        Self {
            signatures: compiled,
        }
    }

    /// Analyze a full list of scanned entries.
    ///
    /// `now_unix` drives age-based rules. Suggestions are de-duplicated so that
    /// when a whole directory is recommended, individual files beneath it are
    /// not also listed.
    #[must_use]
    pub fn analyze(&self, entries: &[FileEntry], now_unix: i64) -> AnalysisReport {
        // First pass, in parallel: for each entry find the first matching
        // signature index. Each path is normalized exactly once.
        let mut matches: Vec<(&FileEntry, usize)> = entries
            .par_iter()
            .filter_map(|entry| {
                if crate::safety::is_protected(&entry.path) {
                    return None;
                }
                let norm_path = normalize(&entry.path.to_string_lossy());
                let name_lower = norm_path
                    .trim_end_matches('\\')
                    .rsplit('\\')
                    .next()
                    .unwrap_or("");
                self.signatures
                    .iter()
                    .position(|s| {
                        s.rule.matches(
                            &norm_path,
                            name_lower,
                            entry.is_dir,
                            entry.modified,
                            now_unix,
                        )
                    })
                    .map(|idx| (entry, idx))
            })
            .collect();

        // Second pass: prune matches nested under an already-accepted directory,
        // so a cache directory is suggested once rather than per-file. Sorting by
        // path length guarantees ancestors are processed before descendants.
        matches.sort_by_key(|(e, _)| e.path.as_os_str().len());
        let mut accepted_dirs: Vec<PathBuf> = Vec::new();
        let mut accepted: Vec<(&FileEntry, usize)> = Vec::new();
        for (entry, sig_idx) in matches {
            if accepted_dirs.iter().any(|d| entry.path.starts_with(d)) {
                continue;
            }
            if entry.is_dir {
                accepted_dirs.push(entry.path.clone());
            }
            accepted.push((entry, sig_idx));
        }

        // Size only the accepted directories (usually a handful), and skip the
        // work entirely when the analysis matched only files.
        let dir_index =
            (!accepted_dirs.is_empty()).then(|| DirSizeIndex::build_for(entries, &accepted_dirs));

        let mut suggestions: Vec<Suggestion> = Vec::new();
        for (entry, sig_idx) in accepted {
            let path = &entry.path;
            let size = if entry.is_dir {
                dir_index.as_ref().map_or(0, |idx| idx.size_of(path))
            } else {
                entry.size
            };
            if size == 0 {
                continue;
            }
            let sig = &self.signatures[sig_idx];
            suggestions.push(Suggestion {
                signature_id: sig.id.clone(),
                title: sig.title.clone(),
                category: sig.category.clone(),
                confidence: sig.confidence.label().to_string(),
                path: path.clone(),
                is_dir: entry.is_dir,
                size,
                reason: sig.description.clone(),
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

    #[test]
    fn age_gated_glob_matches_old_files_only() {
        let set = SignatureSet {
            signatures: vec![sig(
                "old-exe",
                Confidence::LikelySafe,
                MatchRule::Glob {
                    patterns: vec!["*.exe".to_string()],
                    within: Some(r"\downloads\".to_string()),
                    min_age_days: Some(90),
                },
            )],
        };
        let now = 1000 * 86_400;
        let entries = vec![
            FileEntry::file("/x/Downloads/old.exe", 10).with_modified(Some(800 * 86_400)),
            FileEntry::file("/x/Downloads/new.exe", 10).with_modified(Some(990 * 86_400)),
        ];
        let report = Analyzer::new(set).analyze(&entries, now);
        assert_eq!(report.suggestions.len(), 1);
        assert!(report.suggestions[0].path.ends_with("old.exe"));
    }
}
