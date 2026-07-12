//! The cleanup **rule engine**: signatures that describe categories of files
//! safe to reclaim, and the matching primitives that detect them.
//!
//! Signatures are data, not code — they live as TOML in the `knowledge-base/`
//! directory so the community can contribute new categories without touching
//! Rust. A default set is embedded in the binary so purify works out of the box;
//! additional signatures can be loaded from a directory at runtime.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// How confident we are that a matched item is safe to reclaim.
///
/// This is the single most important piece of information purify gives the user:
/// it is the difference between "one-click clean" and "look before you leap".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Confidence {
    /// Reinstallable/regenerable with no user impact (caches, temp files).
    Safe,
    /// Almost always safe, but with a plausible edge case (old installers).
    LikelySafe,
    /// Real reclaimable space, but the user should confirm (node_modules).
    ReviewNeeded,
}

impl Confidence {
    /// A stable lowercase label for display and JSON.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Confidence::Safe => "safe",
            Confidence::LikelySafe => "likely-safe",
            Confidence::ReviewNeeded => "review-needed",
        }
    }

    /// Ranking for sorting (safest first).
    #[must_use]
    pub fn rank(self) -> u8 {
        match self {
            Confidence::Safe => 0,
            Confidence::LikelySafe => 1,
            Confidence::ReviewNeeded => 2,
        }
    }
}

/// How a signature detects matching paths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum MatchRule {
    /// Match any directory whose final component equals `name`
    /// (case-insensitive), e.g. `node_modules`.
    DirName {
        /// The directory name to match.
        name: String,
    },
    /// Match any path containing `needle` (case-insensitive, `/` and `\` treated
    /// alike), e.g. `\appdata\local\npm-cache`. Matches whole directories.
    PathContains {
        /// The substring to look for in the normalized path.
        needle: String,
    },
    /// Match files by wildcard `patterns` (e.g. `*.tmp`), optionally restricted
    /// to paths containing `within` and older than `min_age_days`.
    Glob {
        /// Wildcard patterns applied to the file name (supports a single `*`).
        patterns: Vec<String>,
        /// Optional path substring the file must be located within.
        #[serde(default)]
        within: Option<String>,
        /// Optional minimum age in days (requires a known modification time).
        #[serde(default)]
        min_age_days: Option<u32>,
    },
}

/// A single cleanup signature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signature {
    /// Stable unique identifier (e.g. `npm-cache`).
    pub id: String,
    /// Human-readable title.
    pub title: String,
    /// Category grouping (e.g. `dev-cache`, `browser-cache`, `system-temp`).
    pub category: String,
    /// Confidence that matched items are safe to reclaim.
    pub confidence: Confidence,
    /// A short explanation of what this is and why it is reclaimable.
    pub description: String,
    /// The detection rule.
    #[serde(rename = "match")]
    pub rule: MatchRule,
}

impl MatchRule {
    /// Whether this rule targets directories (as opposed to individual files).
    #[must_use]
    pub fn targets_directories(&self) -> bool {
        matches!(
            self,
            MatchRule::DirName { .. } | MatchRule::PathContains { .. }
        )
    }

    /// Evaluate the rule against a single scanned entry.
    ///
    /// `now_unix` is the current time; used only by age-restricted globs.
    #[must_use]
    pub fn matches(&self, entry: &crate::FileEntry, now_unix: i64) -> bool {
        match self {
            MatchRule::DirName { name } => {
                entry.is_dir && eq_ignore_case(&last_component(&entry.path), name)
            }
            MatchRule::PathContains { needle } => {
                normalize(&entry.path.to_string_lossy()).contains(&normalize(needle))
            }
            MatchRule::Glob {
                patterns,
                within,
                min_age_days,
            } => {
                if entry.is_dir {
                    return false;
                }
                if let Some(w) = within {
                    if !normalize(&entry.path.to_string_lossy()).contains(&normalize(w)) {
                        return false;
                    }
                }
                if let Some(days) = min_age_days {
                    let Some(modified) = entry.modified else {
                        return false; // age unknown -> do not match
                    };
                    let age_secs = now_unix.saturating_sub(modified);
                    if age_secs < i64::from(*days) * 86_400 {
                        return false;
                    }
                }
                let name = last_component(&entry.path);
                patterns.iter().any(|p| wildcard_match(&name, p))
            }
        }
    }
}

/// A collection of signatures.
#[derive(Debug, Clone, Default)]
pub struct SignatureSet {
    /// The loaded signatures.
    pub signatures: Vec<Signature>,
}

#[derive(Debug, Deserialize)]
struct SignatureFile {
    #[serde(default, rename = "signature")]
    signatures: Vec<Signature>,
}

impl SignatureSet {
    /// The default signature set embedded in the binary.
    ///
    /// This is what makes purify useful with zero configuration.
    #[must_use]
    pub fn builtin() -> Self {
        // Each category lives in its own TOML file under knowledge-base/.
        const FILES: &[(&str, &str)] = &[
            (
                "dev-caches",
                include_str!("../../../knowledge-base/dev-caches.toml"),
            ),
            (
                "app-caches",
                include_str!("../../../knowledge-base/app-caches.toml"),
            ),
            (
                "windows",
                include_str!("../../../knowledge-base/windows.toml"),
            ),
            (
                "downloads",
                include_str!("../../../knowledge-base/downloads.toml"),
            ),
        ];
        let mut signatures = Vec::new();
        for (name, contents) in FILES {
            match toml::from_str::<SignatureFile>(contents) {
                Ok(file) => signatures.extend(file.signatures),
                // A malformed embedded file is a build-time bug; a test enforces
                // that this never happens, so at runtime we simply skip it.
                Err(err) => {
                    tracing::error!(file = name, %err, "embedded signature file failed to parse")
                }
            }
        }
        Self { signatures }
    }

    /// Load additional signatures from every `*.toml` file in `dir`.
    pub fn load_dir(dir: &Path) -> Result<Self> {
        let mut signatures = Vec::new();
        let read = std::fs::read_dir(dir).map_err(|e| Error::io(dir, e))?;
        for entry in read {
            let entry = entry.map_err(|e| Error::io(dir, e))?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }
            let contents = std::fs::read_to_string(&path).map_err(|e| Error::io(&path, e))?;
            let file: SignatureFile =
                toml::from_str(&contents).map_err(|e| Error::InvalidSignature {
                    path: path.clone(),
                    message: e.to_string(),
                })?;
            signatures.extend(file.signatures);
        }
        Ok(Self { signatures })
    }

    /// Number of signatures loaded.
    #[must_use]
    pub fn len(&self) -> usize {
        self.signatures.len()
    }

    /// Whether the set is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.signatures.is_empty()
    }
}

/// Lowercase a string and unify path separators to `\`.
fn normalize(s: &str) -> String {
    s.to_ascii_lowercase().replace('/', "\\")
}

fn eq_ignore_case(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// The final path component as a string, using both separators.
fn last_component(path: &Path) -> String {
    let s = path.to_string_lossy().replace('/', "\\");
    s.trim_end_matches('\\')
        .rsplit('\\')
        .next()
        .unwrap_or("")
        .to_string()
}

/// Minimal wildcard match supporting a single `*` (any run of characters).
/// Case-insensitive. Sufficient for signature patterns like `*.tmp` or `~$*`.
fn wildcard_match(name: &str, pattern: &str) -> bool {
    let name = name.to_ascii_lowercase();
    let pattern = pattern.to_ascii_lowercase();
    match pattern.split_once('*') {
        None => name == pattern,
        Some((prefix, suffix)) => {
            name.len() >= prefix.len() + suffix.len()
                && name.starts_with(prefix)
                && name.ends_with(suffix)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FileEntry;

    #[test]
    fn wildcard_matches_extensions() {
        assert!(wildcard_match("setup.tmp", "*.tmp"));
        assert!(wildcard_match("SETUP.TMP", "*.tmp"));
        assert!(!wildcard_match("setup.exe", "*.tmp"));
        assert!(wildcard_match("anything", "*"));
        assert!(wildcard_match("exact", "exact"));
    }

    #[test]
    fn dirname_rule_matches_directories_only() {
        let rule = MatchRule::DirName {
            name: "node_modules".to_string(),
        };
        assert!(rule.matches(&FileEntry::dir(r"C:\proj\node_modules"), 0));
        assert!(!rule.matches(&FileEntry::file(r"C:\proj\node_modules\x.js", 1), 0));
        assert!(!rule.matches(&FileEntry::dir(r"C:\proj\src"), 0));
    }

    #[test]
    fn path_contains_is_case_and_slash_insensitive() {
        let rule = MatchRule::PathContains {
            needle: r"\AppData\Local\npm-cache".to_string(),
        };
        assert!(rule.matches(&FileEntry::dir(r"C:/Users/me/appdata/local/NPM-CACHE"), 0));
        assert!(!rule.matches(&FileEntry::dir(r"C:\Users\me\Documents"), 0));
    }

    #[test]
    fn glob_respects_within_and_age() {
        let rule = MatchRule::Glob {
            patterns: vec!["*.msi".to_string(), "*.exe".to_string()],
            within: Some(r"\downloads\".to_string()),
            min_age_days: Some(90),
        };
        let now = 1_000 * 86_400; // day 1000
                                  // Old installer in Downloads -> matches.
        let old = FileEntry::file(r"C:\Users\me\Downloads\setup.exe", 500)
            .with_modified(Some(800 * 86_400));
        assert!(rule.matches(&old, now));
        // Recent installer -> too new.
        let recent = FileEntry::file(r"C:\Users\me\Downloads\new.exe", 500)
            .with_modified(Some(990 * 86_400));
        assert!(!rule.matches(&recent, now));
        // Right age but wrong location.
        let elsewhere = FileEntry::file(r"C:\tools\old.exe", 500).with_modified(Some(100 * 86_400));
        assert!(!rule.matches(&elsewhere, now));
        // Unknown age -> does not match (conservative).
        let unknown = FileEntry::file(r"C:\Users\me\Downloads\x.exe", 500);
        assert!(!rule.matches(&unknown, now));
    }

    #[test]
    fn builtin_signatures_load_and_are_plentiful() {
        let set = SignatureSet::builtin();
        assert!(
            set.len() >= 20,
            "expected at least 20 builtin signatures, got {}",
            set.len()
        );
        // IDs must be unique.
        let mut ids: Vec<&str> = set.signatures.iter().map(|s| s.id.as_str()).collect();
        ids.sort_unstable();
        let before = ids.len();
        ids.dedup();
        assert_eq!(before, ids.len(), "signature ids must be unique");
    }
}
