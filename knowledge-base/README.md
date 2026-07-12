# Knowledge base

Community-editable cleanup **signatures** live here as TOML files, loaded by the
rule engine (Phase 2). Each signature describes a category of files that is safe
to quarantine, why it is safe, and its confidence level
(Safe / Likely-Safe / Review-Needed).

The format and the first ~20 categories (dev caches like npm/pip/cargo/Docker,
Windows update leftovers, browser/app caches, old installers in Downloads,
Recycle Bin, dump files, thumbnail cache, …) land in Phase 2. Contributing a new
safe category here requires no Rust.
