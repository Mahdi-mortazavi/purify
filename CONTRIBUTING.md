# Contributing to purify

Thanks for helping build purify. This project lives or dies on **user trust**,
so data safety comes before every other consideration — including development
speed. Please read [`ARCHITECTURE.md`](ARCHITECTURE.md) first; it explains the
principles your change must respect.

## Ground rules

- **Never delete a user's data directly.** All cleanup goes through the
  reversible quarantine. A PR that permanently deletes files will not be merged.
- **No telemetry, no network calls** in the core product. purify is fully
  offline.
- **No `unsafe`** outside `purify-ntfs`. Inside it, every `unsafe` block needs a
  `// SAFETY:` comment.
- **No `unwrap`/`expect`/`panic`** on production paths. Return typed errors.
- Explain **why** in commits and PRs, not just what.

## Development setup

```sh
# Rust stable is pinned via rust-toolchain.toml.
cargo build --workspace
cargo test --workspace
```

## Before you open a PR

Run the same checks CI does:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Optionally, mirror the dependency-license gate:

```sh
cargo install cargo-deny   # once
cargo deny check
```

## Tests

- Unit and integration tests must use a synthetic filesystem (`tempfile`).
- **No test may touch a real user disk** or require administrator privileges.

## Knowledge base contributions

Cleanup signatures (Phase 2 onward) live in `knowledge-base/` as TOML. Adding a
new safe-to-clean category is one of the most valuable contributions you can
make — no Rust required. Each signature must document what it targets, why it is
safe, and its confidence level.
