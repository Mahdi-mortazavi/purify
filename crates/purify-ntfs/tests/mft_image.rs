//! Integration test: run the MFT traversal against a real NTFS filesystem
//! image so the parsing logic is exercised end-to-end on every platform.
//!
//! The fixture `tests/fixtures/testfs.ntfs.gz` is a gzip-compressed NTFS volume
//! created with `mkntfs`, containing a known layout:
//!
//! ```text
//! \alpha.txt              1000 bytes
//! \sub\beta.bin           2000 bytes
//! \sub\deep\gamma.dat       500 bytes
//! ```

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{Cursor, Read};

use flate2::read::GzDecoder;
use purify_core::FileEntry;
use purify_ntfs::aligned::AlignedReader;

fn load_image() -> Vec<u8> {
    let gz = include_bytes!("fixtures/testfs.ntfs.gz");
    let mut decoder = GzDecoder::new(&gz[..]);
    let mut out = Vec::new();
    decoder
        .read_to_end(&mut out)
        .expect("decompress ntfs image");
    out
}

fn collect_entries(image: Vec<u8>) -> Vec<FileEntry> {
    // Route through AlignedReader to exercise it too (sector size 512 here).
    let mut reader = AlignedReader::new(Cursor::new(image), 512);
    let mut entries = Vec::new();
    purify_ntfs::mft::scan_reader(&mut reader, |e| entries.push(e)).expect("scan image");
    entries
}

fn find<'a>(entries: &'a [FileEntry], name: &str) -> Option<&'a FileEntry> {
    entries.iter().find(|e| e.file_name_lossy() == name)
}

#[test]
fn mft_scan_finds_all_files_with_correct_sizes() {
    let entries = collect_entries(load_image());

    let alpha = find(&entries, "alpha.txt").expect("alpha.txt present");
    assert!(!alpha.is_dir);
    assert_eq!(alpha.size, 1000, "alpha.txt size");

    let beta = find(&entries, "beta.bin").expect("beta.bin present");
    assert!(!beta.is_dir);
    assert_eq!(beta.size, 2000, "beta.bin size");

    let gamma = find(&entries, "gamma.dat").expect("gamma.dat present");
    assert!(!gamma.is_dir);
    assert_eq!(gamma.size, 500, "gamma.dat size");
}

#[test]
fn mft_scan_reports_directories_and_nesting() {
    let entries = collect_entries(load_image());

    let sub = find(&entries, "sub").expect("sub dir present");
    assert!(sub.is_dir, "sub is a directory");

    let deep = find(&entries, "deep").expect("deep dir present");
    assert!(deep.is_dir, "deep is a directory");

    // gamma.dat must be nested under sub\deep.
    let gamma = find(&entries, "gamma.dat").expect("gamma present");
    let path = gamma.path.to_string_lossy().replace('/', "\\");
    assert!(path.ends_with(r"sub\deep\gamma.dat"), "nested path: {path}");
}

#[test]
fn mft_scan_total_bytes_match_known_layout() {
    let entries = collect_entries(load_image());
    let total: u64 = entries.iter().filter(|e| !e.is_dir).map(|e| e.size).sum();
    assert_eq!(total, 3500, "1000 + 2000 + 500");
}
