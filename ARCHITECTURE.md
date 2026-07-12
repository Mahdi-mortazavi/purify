# purify — Architecture

This document explains **how purify is structured and why**. It is the map for
contributors and the record of the decisions that keep the project safe, fast,
and trustworthy. Keep it current: a PR that changes the architecture updates
this file in the same change.

## The problem, from first principles

"My C: drive is full and my files are a mess" is really three problems:

1. **Visibility** — the user has no fast, understandable picture of *what* is
   taking space and *why*.
2. **Decision paralysis** — even seeing the files, they don't know what is
   *safe* to delete. Fear of breaking Windows produces inaction. Existing tools
   hand this hardest step back to the user.
3. **Entropy** — clutter is a *flow*, not an event: from the user (Downloads,
   Desktop) and from Windows itself (WinSxS, update files, caches). A one-time
   cleanup without prevention is pointless.

So purify must: make the disk understandable in seconds, make the delete
decision *for* the user with evidence and a confidence level, keep every action
100% reversible, and prevent re-accumulation.

## Non-negotiable principles

These constrain every design decision. If a feature conflicts with one of these,
the feature is wrong.

1. **Never delete directly.** Every cleanup moves items to a reversible
   quarantine with full metadata (original path, date, reason). Permanent
   deletion happens only after a retention window (default 30 days) *with user
   confirmation*.
2. **Total transparency.** For each suggested item: what is it, why is it
   removable, how much space it frees, and a confidence level
   (Safe / Likely-Safe / Review-Needed).
3. **No telemetry, fully offline, permissive license (MIT/Apache-2.0).**
4. **Never touch:** critical system files, the Registry (no registry cleaning —
   it is dangerous and worthless), or in-use files without warning.
5. **Dry-run by default.** Every operation is simulated and reported first, then
   executed only on confirmation.
6. **Performance.** A full NTFS scan targets < 10 s (direct MFT read with admin;
   portable walker fallback without). Background monitor stays under 50 MB RAM.

## Workspace layout

purify is a Cargo workspace. Responsibilities are split so that **all `unsafe`
lives in exactly one crate** and everything else stays portable and unit-testable
against a synthetic filesystem.

```
crates/
  purify-core   Pure Rust. Data models, Scanner trait, rule engine,
                dedup (BLAKE3), quarantine logic. #![forbid(unsafe_code)].
  purify-ntfs   Raw NTFS/volume access. The ONLY crate allowed unsafe,
                confined to opening a volume handle and reading sectors.
                Windows-only implementation; portable stub elsewhere.
  purify-cli    The `purify` binary. Portable, #![forbid(unsafe_code)].
knowledge-base/ Community-editable cleanup signatures (TOML). Filled in Phase 2.
```

Planned later: `purify-desktop` (Tauri UI, Phase 4), `purify-guardian`
(background monitor service, Phase 5).

### Why `purify-ntfs`, not `purify-mft`

The MFT is one structure *inside* NTFS. The crate boundary is "raw filesystem
access" — opening the volume, reading the boot sector, traversing records — so
the broader name is the honest one.

### Why the `ntfs` crate instead of a hand-rolled parser

MFT *record parsing* is delegated to the safe [`ntfs`](https://crates.io/crates/ntfs)
crate. That shrinks our own `unsafe` surface to a few dozen auditable lines
(acquire a `\\.\C:` handle, read raw sectors) instead of thousands of lines of
brittle parser. Safer, faster to ship, and faithful to the "minimize unsafe"
principle.

## The unsafe boundary

- Every crate except `purify-ntfs` sets `#![forbid(unsafe_code)]` via the
  workspace lint table.
- `purify-ntfs` relaxes this to `deny`, so each `unsafe` block must be explicitly
  `#[allow(unsafe_code)]`-ed and carry a `// SAFETY:` comment. `clippy`'s
  `undocumented_unsafe_blocks` enforces the comment.

## Scanning strategy

`purify_core::Scanner` is the abstraction. Two implementations:

- **`MftScanner`** (`purify-ntfs`): direct MFT reads, fast, Windows + admin.
- **Portable walker** (Phase 1): parallel directory walk (`jwalk`) for
  non-Windows or non-admin. Correct everywhere, just slower.

The CLI picks the fastest available strategy and falls back automatically.
Scanning streams entries through a callback so peak memory stays bounded on
drives with tens of millions of files.

## Quarantine design

- **Per-volume roots.** A quarantine root lives on each drive so same-volume
  removal is an instant `rename` (a metadata op), never a multi-gigabyte copy.
  Cross-volume moves fall back to copy+delete only when unavoidable.
- **SQLite metadata** (`rusqlite`, bundled) records original path, timestamp,
  reason, and confidence for complete `undo` and scheduled purge.

## Error handling & observability

- Libraries return typed errors (`thiserror`); binaries wrap in `anyhow`.
- No `unwrap`/`expect`/`panic` on production paths — enforced as clippy warnings
  across the workspace.
- Structured logging via `tracing`; verbosity controlled by `-v` flags and
  `RUST_LOG`.

## Development phases

Each phase is independently usable and tested.

- **Phase 0 — Skeleton** ✅: workspace, safety lints, CI, docs.
- **Phase 1 — Scanner** ✅: MFT read + portable fallback, disk-usage tree, CLI
  top-N.
- **Phase 2 — Knowledge & analysis** ✅: rule engine + 30+ signature categories,
  cleanup suggestions with confidence levels.
- **Phase 3 — Quarantine & safe cleanup** ✅: quarantine + undo + dry-run +
  scheduled purge.
- **Phase 4 — Desktop UI (Tauri)** ✅: treemap, suggestion list, one-click safe
  cleanup, quarantine/undo screen.
- **Phase 5 — Organizer + Guardian** ✅: organization rules, disk-space monitor.
- **Phase 6 — Release & polish** ✅: GitHub Actions Windows x64 release
  workflow, installers, benchmarks methodology, contribution docs.

### Component map (as built)

`purify-core` modules: `scan` (portable walker) · `usage` (disk tree +
directory-size index) · `rules` (signatures + matching) · `suggest` (analyzer) ·
`quarantine` (SQLite-backed, reversible) · `organize` (rule-driven moves + undo)
· `guardian` (disk pressure) · `safety` (protected paths) · `fsutil` (shared
move/copy). `purify-ntfs`: `mft` (NTFS traversal) + `aligned` (sector-aligned
volume reads). `purify-cli`: `scan`/`analyze`/`clean`/`list`/`restore`/`purge`/
`organize`/`guard`. `purify-desktop`: Tauri 2 app.
