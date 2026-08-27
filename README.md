# purify

<div align="center">

**See what is filling your disk. Reclaim it without fear.**

Fast, private and reversible disk cleanup for Windows — built with Rust and Tauri.

[![CI](https://github.com/Mahdi-mortazavi/purify/actions/workflows/ci.yml/badge.svg)](https://github.com/Mahdi-mortazavi/purify/actions/workflows/ci.yml)
[![Latest release](https://img.shields.io/github/v/release/Mahdi-mortazavi/purify?sort=semver&color=0a84ff)](https://github.com/Mahdi-mortazavi/purify/releases/latest)
[![License: MIT](https://img.shields.io/badge/license-MIT-34c759.svg)](LICENSE)
[![English](https://img.shields.io/badge/docs-English-0a84ff)](README.md)
[![فارسی](https://img.shields.io/badge/docs-فارسی-5e5ce6)](README.fa.md)

[Download for Windows](https://github.com/Mahdi-mortazavi/purify/releases/latest) · [Report a bug](https://github.com/Mahdi-mortazavi/purify/issues/new) · [Contribute](CONTRIBUTING.md)

</div>

Your drive is full. The usual tools either show you a wall of folders or ask you to delete files you cannot identify. **purify closes that gap:** understand what is large, decide what is safe, move it to a reversible quarantine, and keep the next cleanup from becoming a fire drill.

Nothing is deleted by default. Every suggestion has a confidence level and a plain-language reason.

## What you get

- **Disk Map** — scan a drive in seconds with a direct NTFS/MFT reader (or a portable fallback) and see the largest consumers.
- **Safe Cleanup** — 30+ built-in signatures for caches, leftovers and developer clutter, ranked as Safe, Likely Safe or Review Needed.
- **Quarantine + undo** — cleanup moves items out of the way; restore them anytime. Permanent purge is explicit and delayed.
- **Organizer** — preview and sort loose files in Downloads with one-command undo.
- **Disk Guardian** — know when storage pressure is becoming a problem.
- **Private by design** — offline, no telemetry, no registry edits and no protected-system-file surprises.

## Start in 60 seconds

1. Download the latest [Windows release](https://github.com/Mahdi-mortazavi/purify/releases/latest).
2. Open **purify** and choose a drive.
3. Press **Scan** to understand the space, then **Analyze** to get suggestions.
4. Review the confidence and reason for each item. **Clean** moves approved items to quarantine.

The CLI is useful when you want a scriptable, read-only view:

```powershell
purify scan C:\ --top 20
purify analyze C:\Users\me
purify clean C:\Users\me                 # preview only
purify clean C:\Users\me --apply         # reversible quarantine
purify restore <id>
```

## Why it feels fast

With administrator rights, purify reads the NTFS Master File Table directly instead of walking every file. Without admin it uses a parallel walker. Matching is compiled once, paths are normalized once, and directory sizes are computed only when needed.

On a representative 7,400-file profile, release builds measured roughly **20 ms for `scan`** and **28 ms for `analyze`**. Results depend on the drive and hardware; the benchmark method is documented in [`ARCHITECTURE.md`](ARCHITECTURE.md).

## For developers

The workspace is deliberately split by risk:

| Crate | Responsibility |
| --- | --- |
| `purify-core` | Rules, suggestions, quarantine, organizer and guardian; safe Rust only |
| `purify-ntfs` | The only crate with raw volume/MFT access |
| `purify-cli` | Scriptable command-line interface |
| `purify-desktop` | Tauri 2 desktop UI |
| `knowledge-base/` | Community-editable cleanup signatures |

```powershell
git clone https://github.com/Mahdi-mortazavi/purify.git
cd purify
cargo test --workspace --exclude purify-desktop
cargo fmt --all --check
cargo clippy --workspace --exclude purify-desktop --all-targets -- -D warnings
```

The desktop crate needs the platform WebView SDK. CI runs lint, tests, desktop builds on Windows and Ubuntu, and dependency policy checks on every pull request.

## Contributing

The best first contribution is often a cleanup signature: it needs domain knowledge, not Rust. Read [`CONTRIBUTING.md`](CONTRIBUTING.md), explain the safety boundary, add a test when behavior changes, and keep the diff focused.

Ideas, bug reports and UX feedback are welcome in [Issues](https://github.com/Mahdi-mortazavi/purify/issues). Please include Windows version, whether the app was elevated, the command or screen involved, and a safe reproduction path.

## Roadmap

- More Windows-safe signatures and better explanations
- Winget/MSIX distribution and signed installers
- Accessibility and keyboard-first polish across the desktop UI

## License

MIT © Mahdi Mortazavi. See [`LICENSE`](LICENSE).
