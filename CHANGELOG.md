# Changelog

All notable changes to purify are documented here.

## v0.1.1 — QA & hardening

A full test-and-debug pass (aggressive CLI exercise testing plus independent
code review of the core, NTFS, CLI, and desktop layers). Every fix below ships
with a regression test where testable.

### Correctness
- **Scanner:** `scan <subdir> --mft` previously reported whole-*volume* totals
  (the MFT reader always traverses from the volume root) while only listing
  consumers under the requested subdirectory. Entries are now filtered to the
  requested subtree with a **case-insensitive** prefix check (the MFT reports
  on-disk case, which can differ from the user's typed path on case-insensitive
  Windows volumes).
- **Filesystem moves:** `is_cross_device` matched raw OS error `17` on all
  platforms, but `17` is `EEXIST` on Unix (only `ERROR_NOT_SAME_DEVICE` on
  Windows). A rename that failed because the destination existed could be
  misread as cross-volume and silently merged/overwritten. The check is now
  gated per-OS (`EXDEV` = 18 on Unix, `17` on Windows only).

### Safety
- **Protected paths are now drive-agnostic.** System locations
  (`\Windows`, `\Program Files`, boot, recovery, …) are protected on **any**
  drive letter, not just `C:` (Windows can be installed on D:, programs
  relocated, etc.).
- **Surgical safe-cache carve-outs.** Well-known OS caches that Windows itself
  regenerates and that its Disk Cleanup removes — `\Windows\Temp`,
  `\Windows\Prefetch`, `\Windows\Logs`, `SoftwareDistribution\Download`,
  Delivery Optimization, and WER — are now reclaimable, while `System32`,
  `WinSxS`, and boot files stay fully protected. This also makes the previously
  unreachable prefetch / Windows-Update / WER signatures actually fire.

### Robustness
- **Quarantine is now atomic.** If recording a move in the database failed, the
  file was left stranded in the blob area with no way to list or restore it. The
  move is now rolled back on a failed insert, so the operation is all-or-nothing.
- Quarantine ids mix in sub-second entropy so two runs in the same wall-clock
  second can't collide (and a collision would now surface cleanly rather than
  strand a file).

### Desktop UX & performance
- Heavy commands (`scan`, `analyze`, `clean`) now run **off the webview main
  thread** (`async` + `spawn_blocking`), so the UI stays responsive during long
  scans.
- File paths are **HTML-escaped** before rendering, so names containing `&`,
  `<`, or `>` display correctly instead of corrupting the view.
- The Suggestions tab no longer shows stale quarantine content after switching
  back from the Quarantine tab (the last analysis is cached and restored).
- Clean buttons disable and show a "Cleaning…" state during the operation
  (no accidental double-submit).
- A failed disk-space refresh resets the meter instead of showing stale values.
- Fixed the malformed default scan path (`C:\\` → `C:\`).

### CLI polish
- `scan --top 0` now shows the single largest consumer instead of an empty list.

## v0.1.0 — first release

Initial release: phases 1–6. NTFS MFT scanner with portable fallback, a
32-signature rule engine with confidence levels, reversible SQLite-backed
quarantine with undo and scheduled purge, a Tauri 2 desktop UI, a file
organizer, a disk-space guardian, and a GitHub Actions Windows x64 release.
