## What changed?

<!-- One sentence describing the user-visible outcome. -->

## Why?

<!-- Link the issue and explain the user problem. -->

Closes #

## Safety and UX checklist

- [ ] Cleanup remains reversible; no direct deletion was introduced.
- [ ] Offline/privacy guarantees are unchanged.
- [ ] Error and empty states remain understandable.
- [ ] UI changes include a screenshot or short recording.
- [ ] New cleanup rules explain their match, confidence and safety boundary.

## Verification

- [ ] `cargo fmt --all --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] I removed personal paths and private data from this PR.
