//! Purify desktop application (Tauri 2).
//!
//! A thin, safe bridge between the purify core engine and a lightweight web UI.
//! All heavy lifting (scanning, analysis, quarantine, organization) lives in
//! `purify-core`; the commands here just adapt those APIs to the frontend and
//! serialize results to JSON.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};

use purify_core::scan::Scanner;
use purify_core::{
    guardian, AnalysisReport, Analyzer, Confidence, FileEntry, GuardianReport, ItemStatus,
    QuarantineItem, QuarantineRequest, QuarantineStore, SignatureSet, SpaceInfo, Thresholds,
    UsageCollector, UsageReport, WalkScanner,
};

/// Current time as Unix seconds.
fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Open the per-user quarantine store (DB + blob area under the data dir).
fn open_store() -> Result<QuarantineStore, String> {
    let base = dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("purify");
    QuarantineStore::open(&base.join("quarantine.db"), &base.join("quarantine"))
        .map_err(|e| e.to_string())
}

/// Collect all entries under a path with the portable walker.
fn collect_entries(root: &Path) -> Result<Vec<FileEntry>, String> {
    let mut entries = Vec::new();
    WalkScanner::new()
        .scan(root, &mut |e| entries.push(e))
        .map_err(|e| e.to_string())?;
    Ok(entries)
}

/// Scan a path and return its largest consumers (for the treemap).
///
/// `async` + `spawn_blocking` keeps the potentially long, CPU-bound scan off the
/// webview's main thread so the UI stays responsive.
#[tauri::command]
async fn scan_path(path: String, top: usize) -> Result<UsageReport, String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<UsageReport, String> {
        let root = PathBuf::from(&path);
        if !root.exists() {
            return Err(format!("path does not exist: {path}"));
        }
        let mut collector = UsageCollector::new(&root);
        WalkScanner::new()
            .scan(&root, &mut |e| collector.record(&e))
            .map_err(|e| e.to_string())?;
        Ok(collector.into_report(top.max(1)))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Analyze a path and return cleanup suggestions with confidence levels.
#[tauri::command]
async fn analyze_path(path: String) -> Result<AnalysisReport, String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<AnalysisReport, String> {
        let root = PathBuf::from(&path);
        if !root.exists() {
            return Err(format!("path does not exist: {path}"));
        }
        let entries = collect_entries(&root)?;
        Ok(Analyzer::new(SignatureSet::builtin()).analyze(&entries, now_unix()))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Result of a clean operation.
#[derive(serde::Serialize)]
struct CleanOutcome {
    moved: usize,
    reclaimed: u64,
    skipped: usize,
}

/// Clean a path: quarantine suggestions at or below `min_confidence`.
///
/// `min_confidence` is one of "safe", "likely-safe", "review-needed".
#[tauri::command]
async fn clean_path(path: String, min_confidence: String) -> Result<CleanOutcome, String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<CleanOutcome, String> {
        let root = PathBuf::from(&path);
        if !root.exists() {
            return Err(format!("path does not exist: {path}"));
        }
        let max_rank = label_rank(&min_confidence);
        let entries = collect_entries(&root)?;
        let report = Analyzer::new(SignatureSet::builtin()).analyze(&entries, now_unix());

        let store = open_store()?;
        let now = now_unix();
        let mut outcome = CleanOutcome {
            moved: 0,
            reclaimed: 0,
            skipped: 0,
        };
        for s in &report.suggestions {
            if label_rank(&s.confidence) > max_rank {
                continue;
            }
            let req = QuarantineRequest {
                original_path: s.path.clone(),
                size: s.size,
                is_dir: s.is_dir,
                reason: s.reason.clone(),
                signature_id: Some(s.signature_id.clone()),
                confidence: Some(s.confidence.clone()),
            };
            match store.quarantine(&req, now) {
                Ok(_) => {
                    outcome.moved += 1;
                    outcome.reclaimed += s.size;
                }
                Err(_) => outcome.skipped += 1,
            }
        }
        Ok(outcome)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// List items currently held in quarantine.
#[tauri::command]
fn list_quarantine() -> Result<Vec<QuarantineItem>, String> {
    let store = open_store()?;
    let items = store
        .list(Some(ItemStatus::Quarantined))
        .map_err(|e| e.to_string())?;
    Ok(items)
}

/// Restore a quarantined item to its original location.
#[tauri::command]
fn restore_item(id: String) -> Result<(), String> {
    open_store()?.restore(&id).map_err(|e| e.to_string())
}

/// Permanently purge a quarantined item.
#[tauri::command]
fn purge_item(id: String) -> Result<(), String> {
    open_store()?.purge(&id).map_err(|e| e.to_string())
}

/// Report disk-space pressure for the volume hosting `path`.
#[tauri::command]
fn guard_path(path: String) -> Result<GuardianReport, String> {
    let root = PathBuf::from(&path);
    // The path may be a file or may not exist yet; probe the nearest ancestor
    // that does, which shares the same volume.
    let mut probe = root.as_path();
    while !probe.exists() {
        match probe.parent() {
            Some(parent) => probe = parent,
            None => return Err(format!("no accessible volume for {path}")),
        }
    }
    let total = fs2::total_space(probe).map_err(|e| e.to_string())?;
    let available = fs2::available_space(probe).map_err(|e| e.to_string())?;
    Ok(guardian::evaluate(
        SpaceInfo { total, available },
        Thresholds::default(),
    ))
}

/// Rank a confidence label (lower = safer).
fn label_rank(label: &str) -> u8 {
    match label {
        "safe" => Confidence::Safe.rank(),
        "likely-safe" => Confidence::LikelySafe.rank(),
        _ => Confidence::ReviewNeeded.rank(),
    }
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            scan_path,
            analyze_path,
            clean_path,
            list_quarantine,
            restore_item,
            purge_item,
            guard_path,
        ])
        .run(tauri::generate_context!())
        .expect("error while running purify desktop");
}
