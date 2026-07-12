# purify

**An ultra-fast, intelligent disk cleanup and organization utility for Windows,
built in Rust.** Reclaim space on your C: drive and tame file clutter — with
zero fear, because every action is 100% reversible.

purify closes the full loop that other tools leave open —
**understand → decide → act safely → prevent** — on one non-negotiable
foundation: **nothing is ever deleted directly.**

---

## Why purify

Every existing tool solves only part of the problem:

| Tool | Shows disk | Decides what's safe | Reversible | Organizes | Open source |
|------|:---------:|:-------------------:|:----------:|:---------:|:-----------:|
| WinDirStat / WizTree | ✅ | ❌ | — | ❌ | ~ |
| BleachBit / CCleaner | ❌ | ~ | ❌ | ❌ | ~ |
| Czkawka | ~ | ❌ | ❌ | ❌ | ✅ |
| **purify** | ✅ | ✅ **with confidence levels** | ✅ **quarantine + undo** | ✅ | ✅ MIT |

## Features

- **See your disk in seconds** — a direct NTFS MFT scanner (with a portable
  fallback) shows the biggest space consumers as a ranked list or interactive
  treemap.
- **Know what's safe to delete** — 30+ built-in signatures detect dev caches,
  browser/app caches, Windows update leftovers, old installers, and more, each
  tagged **Safe / Likely-Safe / Review-Needed** with a plain-language reason.
- **Act without fear** — cleanup *moves* items to a reversible quarantine
  (SQLite-tracked) that you can restore any time. Permanent deletion only after
  a retention window, with confirmation. Dry-run by default.
- **Stay tidy** — an organizer files loose downloads into typed folders, with
  preview and one-command undo.
- **Prevent the next crunch** — a guardian reports disk pressure and nudges you
  before the drive fills.
- **Private & safe** — no telemetry, fully offline, never touches the registry
  or protected system files.

## Install

**From a release (Windows x64):** download `purify.exe` (CLI) and/or
`purify-desktop.exe` (GUI) — plus the `.msi` installer when available — from the
[Releases](https://github.com/mahdi-mortazavi/purify/releases) page. Verify with
the published `SHA256SUMS.txt`.

**From source:**

```sh
cargo build --release -p purify-cli        # the `purify` CLI
cargo build --release -p purify-desktop    # the desktop app (needs the platform webview SDK)
```

## CLI usage

```sh
# See what's filling a drive (read-only)
purify scan C:\ --top 20
purify scan C:\ --mft            # prefer the fast NTFS MFT reader (admin)
purify scan . --json             # machine-readable

# Find safe-to-reclaim files, with confidence levels (read-only)
purify analyze C:\Users\me

# Clean — DRY RUN by default; only moves to quarantine with --apply
purify clean C:\Users\me                        # preview
purify clean C:\Users\me --apply                # quarantine "safe" items
purify clean C:\Users\me --apply --min-confidence likely-safe

# Manage the quarantine
purify list                      # what's held
purify restore <id>              # undo a cleanup
purify purge --older-than 30 --yes   # permanent, after retention

# Organize loose files (preview -> apply -> undo)
purify organize C:\Users\me\Downloads
purify organize C:\Users\me\Downloads --apply
purify organize C:\Users\me\Downloads --undo

# Watch disk pressure
purify guard C:\
```

## Benchmarks

purify's speed comes from *architecture*, not micro-optimization: with
administrator rights it reads the NTFS Master File Table directly (like WizTree)
instead of walking the tree file-by-file (like WinDirStat), turning a
minutes-long scan into a seconds-long one. Without admin it falls back to a
parallel walker (`jwalk`).

Methodology (reproducible): `purify scan C:\ --json` reports wall-clock-relevant
totals; compare against WizTree and WinDirStat on the same volume. Representative
numbers will be published per release from CI-measured runs; because they depend
heavily on drive contents and hardware, we report them with the exact machine
spec rather than a single headline figure. Contributions of measured results
(with specs) are welcome.

## Roadmap

| Phase | Scope | Status |
|------:|-------|--------|
| 0 | Workspace skeleton, safety lints, CI, docs | ✅ |
| 1 | MFT scanner + portable fallback, disk tree, CLI top-N | ✅ |
| 2 | Rule engine + 30+ cleanup signatures with confidence | ✅ |
| 3 | Quarantine + undo + dry-run + scheduled purge | ✅ |
| 4 | Tauri desktop UI (treemap, one-click safe cleanup) | ✅ |
| 5 | Organizer + disk-space guardian | ✅ |
| 6 | Windows x64 release via GitHub Actions, packaging, docs | ✅ |

## Workspace

- **`purify-core`** — pure-Rust engine (scan, rules, suggest, quarantine,
  organize, guardian, safety). No `unsafe`.
- **`purify-ntfs`** — direct NTFS/MFT reader; the only crate with raw volume
  access.
- **`purify-cli`** — the `purify` binary.
- **`purify-desktop`** — the Tauri 2 desktop app.
- **`knowledge-base/`** — community-editable cleanup signatures (TOML).

See [`ARCHITECTURE.md`](ARCHITECTURE.md) for the full design and rationale.

## Contributing

Contributions — especially cleanup signatures — are welcome and need no Rust.
Read [`CONTRIBUTING.md`](CONTRIBUTING.md) and [`ARCHITECTURE.md`](ARCHITECTURE.md)
first. Data safety comes before everything.

## Packaging notes

- **Installers:** the release workflow builds MSI and NSIS installers via the
  Tauri bundler on Windows (best-effort; the raw `.exe`s always ship).
- **winget / MSIX:** planned. The MSI is the basis for a winget manifest; an
  MSIX package can wrap the same binaries.
- **Code signing:** release binaries are unsigned today. Signing requires an
  Authenticode certificate; the workflow is structured so a signing step can be
  added once a certificate is available (SmartScreen reputation otherwise builds
  over time).

## License

MIT © Mahdi Mortazavi. See [`LICENSE`](LICENSE).
