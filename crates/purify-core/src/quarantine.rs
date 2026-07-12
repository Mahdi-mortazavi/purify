//! Reversible **quarantine** — purify's most important safety mechanism.
//!
//! purify never deletes directly. Every cleanup *moves* the target into a
//! managed quarantine and records rich metadata (original path, size, reason,
//! confidence, timestamp) in a SQLite database. From there an item can be
//! restored to its exact original location, or permanently purged after a
//! retention window with the user's confirmation.
//!
//! Moves prefer `rename` (an instant metadata operation) and fall back to a
//! recursive copy only when the quarantine lives on a different volume than the
//! source. Protected system paths are refused outright.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use rusqlite::Connection;
use serde::Serialize;

use crate::error::{Error, Result};
use crate::fsutil::{move_path, remove_path};
use crate::safety;

/// Lifecycle status of a quarantined item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ItemStatus {
    /// Currently held in quarantine, restorable.
    Quarantined,
    /// Restored to its original location.
    Restored,
    /// Permanently deleted.
    Purged,
}

impl ItemStatus {
    fn as_str(self) -> &'static str {
        match self {
            ItemStatus::Quarantined => "quarantined",
            ItemStatus::Restored => "restored",
            ItemStatus::Purged => "purged",
        }
    }

    fn parse(s: &str) -> ItemStatus {
        match s {
            "restored" => ItemStatus::Restored,
            "purged" => ItemStatus::Purged,
            _ => ItemStatus::Quarantined,
        }
    }
}

/// A request to quarantine a single path.
#[derive(Debug, Clone)]
pub struct QuarantineRequest {
    /// Absolute path to move into quarantine.
    pub original_path: PathBuf,
    /// Known size in bytes (for reporting).
    pub size: u64,
    /// Whether the path is a directory.
    pub is_dir: bool,
    /// Why it is being quarantined (e.g. signature description).
    pub reason: String,
    /// The signature id that produced this, if any.
    pub signature_id: Option<String>,
    /// The confidence label, if any.
    pub confidence: Option<String>,
}

/// A record of a quarantined item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QuarantineItem {
    /// Unique identifier.
    pub id: String,
    /// Where the item came from (restore target).
    pub original_path: PathBuf,
    /// Where the item is currently stored.
    pub quarantine_path: PathBuf,
    /// Size in bytes.
    pub size: u64,
    /// Whether it is a directory.
    pub is_dir: bool,
    /// Why it was quarantined.
    pub reason: String,
    /// Signature id, if any.
    pub signature_id: Option<String>,
    /// Confidence label, if any.
    pub confidence: Option<String>,
    /// When it was quarantined (Unix seconds).
    pub quarantined_at: i64,
    /// Lifecycle status.
    pub status: ItemStatus,
}

/// The quarantine store: a SQLite metadata database plus a filesystem area for
/// the held blobs.
#[derive(Debug)]
pub struct QuarantineStore {
    conn: Connection,
    /// Base directory under which quarantined blobs are stored.
    root: PathBuf,
    /// Monotonic counter to disambiguate ids created within the same second.
    counter: AtomicU64,
}

impl QuarantineStore {
    /// Open (creating if needed) a quarantine store with its database at
    /// `db_path` and its blob area at `root`.
    pub fn open(db_path: &Path, root: &Path) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
        std::fs::create_dir_all(root).map_err(|e| Error::io(root, e))?;
        let conn = Connection::open(db_path).map_err(map_sql)?;
        let store = Self {
            conn,
            root: root.to_path_buf(),
            counter: AtomicU64::new(0),
        };
        store.init_schema()?;
        Ok(store)
    }

    /// Open an in-memory store for testing, using `root` for blobs.
    pub fn open_in_memory(root: &Path) -> Result<Self> {
        std::fs::create_dir_all(root).map_err(|e| Error::io(root, e))?;
        let conn = Connection::open_in_memory().map_err(map_sql)?;
        let store = Self {
            conn,
            root: root.to_path_buf(),
            counter: AtomicU64::new(0),
        };
        store.init_schema()?;
        Ok(store)
    }

    fn init_schema(&self) -> Result<()> {
        self.conn
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS items (
                    id              TEXT PRIMARY KEY,
                    original_path   TEXT NOT NULL,
                    quarantine_path TEXT NOT NULL,
                    size            INTEGER NOT NULL,
                    is_dir          INTEGER NOT NULL,
                    reason          TEXT NOT NULL,
                    signature_id    TEXT,
                    confidence      TEXT,
                    quarantined_at  INTEGER NOT NULL,
                    status          TEXT NOT NULL
                );",
            )
            .map_err(map_sql)?;
        Ok(())
    }

    /// Move a path into quarantine and record it. `now_unix` is the timestamp to
    /// stamp the record with.
    pub fn quarantine(&self, req: &QuarantineRequest, now_unix: i64) -> Result<QuarantineItem> {
        if safety::is_protected(&req.original_path) {
            return Err(Error::Refused(format!(
                "refusing to quarantine protected path: {}",
                req.original_path.display()
            )));
        }
        if !req.original_path.exists() {
            return Err(Error::Quarantine(format!(
                "source does not exist: {}",
                req.original_path.display()
            )));
        }

        let id = self.new_id(&req.original_path, now_unix);
        let dest = self.blob_path(&id, &req.original_path);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
        move_path(&req.original_path, &dest)?;

        let item = QuarantineItem {
            id,
            original_path: req.original_path.clone(),
            quarantine_path: dest,
            size: req.size,
            is_dir: req.is_dir,
            reason: req.reason.clone(),
            signature_id: req.signature_id.clone(),
            confidence: req.confidence.clone(),
            quarantined_at: now_unix,
            status: ItemStatus::Quarantined,
        };
        self.insert(&item)?;
        Ok(item)
    }

    fn insert(&self, item: &QuarantineItem) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO items
                 (id, original_path, quarantine_path, size, is_dir, reason,
                  signature_id, confidence, quarantined_at, status)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                rusqlite::params![
                    item.id,
                    item.original_path.to_string_lossy(),
                    item.quarantine_path.to_string_lossy(),
                    item.size,
                    item.is_dir as i64,
                    item.reason,
                    item.signature_id,
                    item.confidence,
                    item.quarantined_at,
                    item.status.as_str(),
                ],
            )
            .map_err(map_sql)?;
        Ok(())
    }

    /// Restore a quarantined item to its original location.
    pub fn restore(&self, id: &str) -> Result<()> {
        let item = self.get(id)?;
        if item.status != ItemStatus::Quarantined {
            return Err(Error::Quarantine(format!(
                "item {id} is not currently quarantined (status: {:?})",
                item.status
            )));
        }
        if item.original_path.exists() {
            return Err(Error::Quarantine(format!(
                "cannot restore {id}: original path already exists: {}",
                item.original_path.display()
            )));
        }
        if let Some(parent) = item.original_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
        move_path(&item.quarantine_path, &item.original_path)?;
        self.set_status(id, ItemStatus::Restored)?;
        Ok(())
    }

    /// Permanently delete a quarantined item's blob and mark it purged.
    pub fn purge(&self, id: &str) -> Result<()> {
        let item = self.get(id)?;
        if item.status == ItemStatus::Quarantined {
            remove_path(&item.quarantine_path)?;
        }
        self.set_status(id, ItemStatus::Purged)?;
        Ok(())
    }

    /// Purge all items older than `retention_days`. Returns the purged ids.
    pub fn purge_expired(&self, retention_days: u32, now_unix: i64) -> Result<Vec<String>> {
        let cutoff = now_unix.saturating_sub(i64::from(retention_days) * 86_400);
        let expired: Vec<String> = self
            .list(Some(ItemStatus::Quarantined))?
            .into_iter()
            .filter(|i| i.quarantined_at <= cutoff)
            .map(|i| i.id)
            .collect();
        for id in &expired {
            self.purge(id)?;
        }
        Ok(expired)
    }

    /// Fetch a single item by id.
    pub fn get(&self, id: &str) -> Result<QuarantineItem> {
        self.conn
            .query_row(
                "SELECT id, original_path, quarantine_path, size, is_dir, reason,
                        signature_id, confidence, quarantined_at, status
                 FROM items WHERE id = ?1",
                [id],
                row_to_item,
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    Error::Quarantine(format!("no such quarantine item: {id}"))
                }
                other => map_sql(other),
            })
    }

    /// List items, optionally filtered by status.
    pub fn list(&self, status: Option<ItemStatus>) -> Result<Vec<QuarantineItem>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, original_path, quarantine_path, size, is_dir, reason,
                        signature_id, confidence, quarantined_at, status
                 FROM items ORDER BY quarantined_at DESC",
            )
            .map_err(map_sql)?;
        let rows = stmt
            .query_map([], row_to_item)
            .map_err(map_sql)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(map_sql)?;
        Ok(match status {
            Some(want) => rows.into_iter().filter(|i| i.status == want).collect(),
            None => rows,
        })
    }

    fn set_status(&self, id: &str, status: ItemStatus) -> Result<()> {
        self.conn
            .execute(
                "UPDATE items SET status = ?1 WHERE id = ?2",
                rusqlite::params![status.as_str(), id],
            )
            .map_err(map_sql)?;
        Ok(())
    }

    /// Deterministic-enough unique id from the path, time, and a counter.
    fn new_id(&self, original: &Path, now_unix: i64) -> String {
        let n = self.counter.fetch_add(1, Ordering::Relaxed);
        let mut hasher = blake3::Hasher::new();
        hasher.update(original.to_string_lossy().as_bytes());
        hasher.update(&now_unix.to_le_bytes());
        hasher.update(&n.to_le_bytes());
        let hex = hasher.finalize().to_hex();
        hex.as_str()[..16].to_string()
    }

    /// The on-disk destination for a quarantined blob, preserving the file name
    /// for human readability while namespacing by id to avoid collisions.
    fn blob_path(&self, id: &str, original: &Path) -> PathBuf {
        let name = original
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "item".to_string());
        self.root.join(id).join(name)
    }
}

fn row_to_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<QuarantineItem> {
    let original: String = row.get(1)?;
    let quarantine: String = row.get(2)?;
    let is_dir: i64 = row.get(4)?;
    let status: String = row.get(9)?;
    Ok(QuarantineItem {
        id: row.get(0)?,
        original_path: PathBuf::from(original),
        quarantine_path: PathBuf::from(quarantine),
        size: row.get(3)?,
        is_dir: is_dir != 0,
        reason: row.get(5)?,
        signature_id: row.get(6)?,
        confidence: row.get(7)?,
        quarantined_at: row.get(8)?,
        status: ItemStatus::parse(&status),
    })
}

fn map_sql(e: rusqlite::Error) -> Error {
    Error::Quarantine(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        _tmp: tempfile::TempDir,
        store: QuarantineStore,
        work: PathBuf,
    }

    fn fixture() -> Fixture {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path().join("quarantine");
        let work = tmp.path().join("work");
        std::fs::create_dir_all(&work).expect("work dir");
        let store = QuarantineStore::open_in_memory(&root).expect("store");
        Fixture {
            _tmp: tmp,
            store,
            work,
        }
    }

    fn make_file(dir: &Path, name: &str, bytes: usize) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, vec![0u8; bytes]).expect("write");
        p
    }

    fn req(path: &Path, size: u64) -> QuarantineRequest {
        QuarantineRequest {
            original_path: path.to_path_buf(),
            size,
            is_dir: path.is_dir(),
            reason: "test".to_string(),
            signature_id: Some("sig".to_string()),
            confidence: Some("safe".to_string()),
        }
    }

    #[test]
    fn quarantine_moves_file_and_records_it() {
        let fx = fixture();
        let file = make_file(&fx.work, "a.bin", 100);
        let item = fx
            .store
            .quarantine(&req(&file, 100), 1000)
            .expect("quarantine");

        assert!(!file.exists(), "original moved away");
        assert!(item.quarantine_path.exists(), "blob present in quarantine");
        assert_eq!(item.status, ItemStatus::Quarantined);

        let listed = fx.store.list(Some(ItemStatus::Quarantined)).expect("list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, item.id);
    }

    #[test]
    fn restore_returns_file_to_original_location() {
        let fx = fixture();
        let file = make_file(&fx.work, "b.bin", 50);
        let item = fx
            .store
            .quarantine(&req(&file, 50), 1000)
            .expect("quarantine");
        assert!(!file.exists());

        fx.store.restore(&item.id).expect("restore");
        assert!(file.exists(), "restored to original path");
        assert_eq!(std::fs::read(&file).expect("read").len(), 50);

        let got = fx.store.get(&item.id).expect("get");
        assert_eq!(got.status, ItemStatus::Restored);
    }

    #[test]
    fn quarantine_and_restore_a_directory() {
        let fx = fixture();
        let dir = fx.work.join("cache");
        std::fs::create_dir_all(dir.join("sub")).expect("mkdir");
        make_file(&dir, "one.bin", 10);
        make_file(&dir.join("sub"), "two.bin", 20);

        let item = fx
            .store
            .quarantine(&req(&dir, 30), 1000)
            .expect("quarantine dir");
        assert!(!dir.exists());
        fx.store.restore(&item.id).expect("restore dir");
        assert!(dir.join("sub/two.bin").exists(), "nested file restored");
    }

    #[test]
    fn refuses_protected_paths() {
        let fx = fixture();
        let bad = QuarantineRequest {
            original_path: PathBuf::from(r"C:\Windows\System32\kernel32.dll"),
            size: 1,
            is_dir: false,
            reason: "x".to_string(),
            signature_id: None,
            confidence: None,
        };
        let err = fx.store.quarantine(&bad, 1000).unwrap_err();
        assert!(matches!(err, Error::Refused(_)));
    }

    #[test]
    fn purge_deletes_blob_permanently() {
        let fx = fixture();
        let file = make_file(&fx.work, "c.bin", 5);
        let item = fx
            .store
            .quarantine(&req(&file, 5), 1000)
            .expect("quarantine");
        let blob = item.quarantine_path.clone();
        assert!(blob.exists());

        fx.store.purge(&item.id).expect("purge");
        assert!(!blob.exists(), "blob deleted");
        assert_eq!(
            fx.store.get(&item.id).expect("get").status,
            ItemStatus::Purged
        );
    }

    #[test]
    fn purge_expired_respects_retention_window() {
        let fx = fixture();
        let old = make_file(&fx.work, "old.bin", 5);
        let fresh = make_file(&fx.work, "fresh.bin", 5);
        // Quarantined 40 days ago and now.
        let day = 86_400i64;
        let now = 100 * day;
        let old_item = fx
            .store
            .quarantine(&req(&old, 5), now - 40 * day)
            .expect("old");
        let fresh_item = fx.store.quarantine(&req(&fresh, 5), now).expect("fresh");

        let purged = fx.store.purge_expired(30, now).expect("purge expired");
        assert_eq!(
            purged,
            vec![old_item.id.clone()],
            "only the 40-day-old item"
        );
        assert_eq!(
            fx.store.get(&old_item.id).expect("get old").status,
            ItemStatus::Purged
        );
        assert_eq!(
            fx.store.get(&fresh_item.id).expect("get fresh").status,
            ItemStatus::Quarantined
        );
    }
}
