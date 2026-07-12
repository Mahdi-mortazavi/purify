# Changelog

All notable changes to purify are documented here.

## v0.1.3 — macOS-inspired desktop redesign

A complete visual overhaul of the desktop app in the style of a modern macOS
application ("Liquid Glass"): translucent frosted-glass materials,
continuous-corner geometry, SF-style typography, a refined indigo→blue accent,
and spring micro-interactions. Fully realized in **both light and dark**.

- **Source-list sidebar** with Disk Map / Cleanup / Quarantine navigation, live
  reclaimable + held badges, and a compact **startup-disk gauge** in the footer.
- **Unified glass toolbar** with a focus-ring path field, primary/secondary
  actions, and an appearance (light/dark) toggle.
- **Disk Map**: an interactive gradient **treemap** with total/file metrics;
  directories and loose files are visually distinguished.
- **Cleanup**: a prominent reclaimable **CTA** with a macOS-style **segmented
  control** (Safe / +Likely / +Review) and confidence-badged suggestion cards.
- **Quarantine**: cards with per-item Restore / Purge.
- Motion respects `prefers-reduced-motion`; the treemap composites on the GPU.

The redesign was validated by rendering the real UI headlessly (Chromium) with
a mocked backend and reviewing screenshots across every view and both themes.
No backend/command changes — the Tauri command surface is unchanged.

## v0.1.2 — performance & full Windows testing

### Performance
- **Analysis is dramatically faster.** Signatures are now *compiled* once
  (needles/patterns pre-lowercased), each scanned path is normalized exactly
  once instead of once per signature, and per-file matching runs in **parallel
  across CPU cores** (rayon). On a realistic ~7,400-file user profile, `analyze`
  runs in ~28 ms and `scan` in ~20 ms (release build).
- **Directory sizing is lazy and targeted.** The recursive directory-size index
  is skipped entirely when an analysis matches only files, and otherwise sizes
  **only the handful of accepted directories** (allocation-free ancestor
  lookups) instead of indexing every ancestor of every file.
- **Full LTO** (`lto = "fat"`) on release builds for maximum runtime speed.
- *Note on GPU:* disk scanning is I/O- and string-bound, so a GPU offers no
  speedup there — the honest GPU win is the desktop UI, whose treemap now renders
  on the WebView2 hardware-accelerated compositor layer with smooth,
  transform-only (60fps) animations that respect `prefers-reduced-motion`.

### Testing (Windows)
- New **end-to-end CLI integration test** (`tests/e2e.rs`) drives the real
  `purify` binary through scan → analyze → clean → list → restore → purge →
  organize → guard, and runs on the **Windows** CI runner (and Linux), with an
  isolated quarantine store.
- New **live-volume MFT probe** validated on the Windows CI runner: opens the
  real `\\.\C:` device handle, sector-aligns, and parses NTFS — the one code
  path that could not run on a non-Windows host. Also exercised on every
  platform against the committed NTFS image.

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
