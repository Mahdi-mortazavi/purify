# purify

**An ultra-fast, intelligent disk cleanup and organization utility for Windows,
built in Rust.** Reclaim space on your C: drive and tame file clutter — with
zero fear, because every action is 100% reversible.

> ⚠️ **Status: Phase 0 (project skeleton).** The workspace, safety guarantees,
> CI, and architecture are in place. Real scanning arrives in Phase 1. See the
> roadmap below.

## Why purify

Every existing tool solves only part of the problem. Disk analyzers (WinDirStat,
WizTree) *show* you the mess but leave the scary decisions to you. Cleaners
(BleachBit, CCleaner) delete *permanently* with no understanding of your system —
and some come with ads, telemetry, or a history of security incidents. Finders
(Czkawka) locate duplicates but don't understand Windows and can't undo.

purify closes the full loop — **understand → decide → act safely → prevent** —
on one non-negotiable foundation: **nothing is ever deleted directly.**

## Principles

- **Reversible by design.** Cleanup moves files to a quarantine with full
  metadata; permanent removal happens only after a retention window, with your
  confirmation.
- **Transparent.** Every suggestion tells you what it is, why it's removable, how
  much it frees, and a confidence level (Safe / Likely-Safe / Review-Needed).
- **Private.** No telemetry. Fully offline. MIT licensed.
- **Safe by default.** Dry-run first. Never touches the Registry or critical
  system files.
- **Fast.** Targets a sub-10-second full NTFS scan via direct MFT reads, with a
  portable fallback.

## Roadmap

| Phase | Scope | Status |
|------:|-------|--------|
| 0 | Workspace skeleton, safety lints, CI, docs | ✅ current |
| 1 | MFT scanner + portable fallback, disk tree, CLI top-N | ⏳ |
| 2 | Rule engine + cleanup signatures (≥ 20 categories) | ⏳ |
| 3 | Quarantine + undo + dry-run + scheduled purge | ⏳ |
| 4 | Tauri desktop UI (treemap, one-click safe cleanup) | ⏳ |
| 5 | Organizer + background Guardian monitor | ⏳ |
| 6 | Installer (MSIX/winget), signed binaries, benchmarks | ⏳ |

## Workspace

- **`purify-core`** — pure-Rust engine (models, scanner trait, rules,
  quarantine). No `unsafe`.
- **`purify-ntfs`** — isolated raw NTFS/volume access; the only crate with
  `unsafe`.
- **`purify-cli`** — the `purify` binary.

See [`ARCHITECTURE.md`](ARCHITECTURE.md) for the full design and rationale.

## Build & run

```sh
cargo build --workspace
cargo run -p purify-cli -- scan C:\
cargo run -p purify-cli -- --help
```

## Contributing

Contributions — especially cleanup signatures — are welcome. Read
[`CONTRIBUTING.md`](CONTRIBUTING.md) and [`ARCHITECTURE.md`](ARCHITECTURE.md)
first. Data safety comes before everything.

## License

MIT © Mahdi Mortazavi. See [`LICENSE`](LICENSE).
