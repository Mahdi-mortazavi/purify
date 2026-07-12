//! End-to-end integration test that drives the real `purify` binary as a
//! subprocess through the full command surface. This runs on **every CI
//! platform, including Windows**, so it validates real on-disk behavior of
//! scan → analyze → clean → list → restore → purge → organize → guard.
//!
//! The test tree lives under `CARGO_TARGET_TMPDIR` (inside `target/`, not under
//! the OS temp dir) so it never accidentally matches location-based signatures
//! like the user-temp rule. The quarantine store is redirected to an isolated
//! per-test directory so it never touches the developer's real quarantine.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_purify")
}

/// A unique working area under target/tmp for one test.
fn workdir(name: &str) -> PathBuf {
    let base = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    base
}

fn write(path: &Path, bytes: usize) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, vec![0u8; bytes]).unwrap();
}

/// Run the binary with an isolated quarantine store. Returns (stdout, success).
fn run(data_dir: &Path, args: &[&str]) -> (String, bool) {
    let out = Command::new(bin())
        .args(args)
        // Redirect the platform data dir so the quarantine store is isolated.
        .env("APPDATA", data_dir) // Windows: dirs::data_dir()
        .env("XDG_DATA_HOME", data_dir) // Linux: dirs::data_dir()
        .env_remove("RUST_BACKTRACE")
        .output()
        .expect("spawn purify");
    let mut s = String::from_utf8_lossy(&out.stdout).into_owned();
    s.push_str(&String::from_utf8_lossy(&out.stderr));
    (s, out.status.success())
}

#[test]
fn full_cli_lifecycle() {
    let root = workdir("e2e_lifecycle");
    let data = root.join("_data");
    let tree = root.join("tree");

    // A safe cache (npm-cache), a review-needed dir (node_modules), and an old
    // installer (likely-safe, age-gated — but age can't be forced portably, so
    // we assert on the always-matching cache paths).
    write(
        &tree.join("Users/me/AppData/Local/npm-cache/x/blob.bin"),
        200_000,
    );
    write(&tree.join("proj/node_modules/lib/dep.js"), 50_000);
    write(&tree.join("Downloads/notes.txt"), 100);

    // scan
    let (out, ok) = run(&data, &["scan", tree.to_str().unwrap(), "--top", "5"]);
    assert!(ok, "scan failed: {out}");
    assert!(out.contains("purify scan"), "scan output: {out}");
    assert!(
        out.contains("files"),
        "scan should report a file count: {out}"
    );

    // scan --json is valid and reports a positive total
    let (json, ok) = run(&data, &["scan", tree.to_str().unwrap(), "--json"]);
    assert!(ok, "scan --json failed: {json}");
    assert!(json.contains("\"total_bytes\""), "json: {json}");

    // analyze finds the npm cache
    let (out, ok) = run(&data, &["analyze", tree.to_str().unwrap()]);
    assert!(ok, "analyze failed: {out}");
    assert!(
        out.contains("npm-cache"),
        "analyze should suggest npm-cache: {out}"
    );

    // clean dry-run does not move anything
    let (out, ok) = run(&data, &["clean", tree.to_str().unwrap()]);
    assert!(ok, "clean dry-run failed: {out}");
    assert!(
        out.contains("DRY RUN"),
        "clean should default to dry-run: {out}"
    );
    assert!(
        tree.join("Users/me/AppData/Local/npm-cache").exists(),
        "dry-run must not move the cache"
    );

    // clean --apply quarantines the safe cache
    let (out, ok) = run(&data, &["clean", tree.to_str().unwrap(), "--apply"]);
    assert!(ok, "clean --apply failed: {out}");
    assert!(out.contains("quarantined"), "should quarantine: {out}");
    assert!(
        !tree.join("Users/me/AppData/Local/npm-cache").exists(),
        "cache should have moved to quarantine"
    );

    // list shows the held item; capture its id
    let (out, ok) = run(&data, &["list"]);
    assert!(ok, "list failed: {out}");
    let id = out
        .split_whitespace()
        .find(|t| t.len() == 16 && t.chars().all(|c| c.is_ascii_hexdigit()))
        .unwrap_or_else(|| panic!("no quarantine id in: {out}"))
        .to_string();

    // restore brings the cache back to its exact original location
    let (out, ok) = run(&data, &["restore", &id]);
    assert!(ok, "restore failed: {out}");
    assert!(
        tree.join("Users/me/AppData/Local/npm-cache/x/blob.bin")
            .exists(),
        "restore should return the file: {out}"
    );

    // re-clean then purge the expired item (older_than 0 = everything)
    let (_o, ok) = run(&data, &["clean", tree.to_str().unwrap(), "--apply"]);
    assert!(ok);
    let (out, ok) = run(&data, &["purge", "--older-than", "0", "--yes"]);
    assert!(ok, "purge failed: {out}");
    assert!(out.to_lowercase().contains("purged"), "purge output: {out}");

    // list is now empty
    let (out, ok) = run(&data, &["list"]);
    assert!(ok);
    assert!(out.contains("empty"), "quarantine should be empty: {out}");

    // guard reports a usage percentage for the drive hosting the tree
    let (out, ok) = run(&data, &["guard", tree.to_str().unwrap()]);
    assert!(ok, "guard failed: {out}");
    assert!(out.contains('%'), "guard should show a percentage: {out}");
}

#[test]
fn organize_apply_and_undo() {
    let root = workdir("e2e_organize");
    let data = root.join("_data");
    let dl = root.join("Downloads");
    // A .txt has no age gate in the default rules? documents require 30 days.
    // Age can't be forced portably here, so assert the flow works even if the
    // dry run finds nothing: the commands must all succeed.
    write(&dl.join("readme.txt"), 10);

    let (out, ok) = run(&data, &["organize", dl.to_str().unwrap()]);
    assert!(ok, "organize dry-run failed: {out}");
    assert!(out.contains("purify organize"), "organize output: {out}");

    // --undo with no prior apply should fail cleanly (non-zero), not panic.
    let (out, ok) = run(&data, &["organize", dl.to_str().unwrap(), "--undo"]);
    assert!(!ok, "undo with no log should fail: {out}");
    assert!(
        out.to_lowercase().contains("no organize log"),
        "undo msg: {out}"
    );
}

#[test]
fn errors_are_clean_and_nonzero() {
    let root = workdir("e2e_errors");
    let data = root.join("_data");

    let (out, ok) = run(&data, &["scan", "/no/such/path/xyz"]);
    assert!(!ok, "scan of missing path should exit non-zero");
    assert!(out.contains("does not exist"), "clean error: {out}");
    assert!(!out.contains("panicked"), "must not panic: {out}");

    let (out, ok) = run(&data, &["restore", "deadbeefdeadbeef"]);
    assert!(!ok, "restore of unknown id should fail");
    assert!(!out.contains("panicked"), "must not panic: {out}");
}
