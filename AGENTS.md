# Development Rules

- Treat `/home/osso/.worktrees/pi-rust-source-532f7d12` as read-only. Its pinned revision lives in `docs/porting/source-lock.json`.
- Preserve observable TypeScript behavior one bounded slice at a time. Ask Alessio before any unapproved semantic deviation.
- Typed Rust core APIs intentionally exclude invalid JavaScript runtime input types, per the August 12, 2026 decision. Keep exclusions explicit in specs and traceability.
- Write and run a failing behavioral or differential test before production code.
- Map every ported source statement and branch to proof or an explicit gap.
- Record source bugs as discrepancies. Do not silently fix or preserve them without classification.
- Replace old paths instead of adding fallbacks.
- Pin direct dependencies exactly and document their purpose.
- Run `cargo fmt --check` before every commit.
- Completion requires focused tests, `cargo check --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, readability audit, and independent verification.
