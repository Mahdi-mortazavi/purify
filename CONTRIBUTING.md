# Contributing to Purify

Thank you for helping make disk cleanup safer. Purify is built around one promise: a user should understand a cleanup before it happens and be able to undo it afterwards.

If you prefer Persian, see [`CONTRIBUTING.fa.md`](CONTRIBUTING.fa.md).

## Start with the right-sized change

- **No-code:** improve a cleanup signature in [`knowledge-base/`](knowledge-base/) or clarify a piece of documentation.
- **Good first code:** add a focused test, improve an error message, or polish a small UI interaction.
- **Larger work:** open an issue first so we can agree on the safety boundary and the user experience.

Please keep each pull request narrow. A small, easy-to-review change is more useful than a broad rewrite.

## Non-negotiable safety rules

- Never delete a user's data directly. Cleanup must go through the reversible quarantine.
- Do not add telemetry or network calls to the core product.
- Keep `unsafe` inside `purify-ntfs` only; every unsafe block needs a nearby `// SAFETY:` explanation.
- Avoid `unwrap`, `expect` and `panic` on production paths. Return a typed error instead.
- Tests must use synthetic filesystems and must not touch a real user disk or require administrator privileges.

## Local setup

Rust is pinned by [`rust-toolchain.toml`](rust-toolchain.toml). From the repository root:

```powershell
cargo build --workspace
cargo test --workspace
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
```

The desktop crate also needs the platform WebView SDK. CI covers the desktop build on Windows and Ubuntu.

## Cleanup signatures

Each TOML rule should make four things obvious:

1. What it matches.
2. Why the content is safe (or why it needs review).
3. Which confidence level it receives.
4. How the behaviour is tested.

When in doubt, choose a narrower match and a lower confidence level. Trust is more important than reclaiming one extra gigabyte.

## Opening a pull request

Use the pull request template and include:

- the user problem and the smallest useful change;
- safety implications and how quarantine/restore behaves;
- tests and checks you ran;
- screenshots or a short recording for UI changes.

Please do not include real personal paths, private data or drive images in issues or pull requests.

## Code review promise

Maintainers aim to acknowledge new issues and pull requests within seven days. Reviews focus first on data safety, then on correctness, clarity and polish. Kind, specific feedback is always welcome.
