# Development Rules

- Treat `/home/osso/.worktrees/pi-rust-source-532f7d12` as read-only. Its pinned revision lives in `docs/porting/source-lock.json`.
- Target: full parity of all packages (decision 2026-10-18). Track every non-test source file in `docs/porting/coverage.md`.
- Port one whole TypeScript module (file or tightly coupled file group) per slice. Ask Alessio before any unapproved semantic deviation.
- Extensions are Rust-native: bundled extensions are ported to a Rust extension API; loading TypeScript extensions is an approved deviation (2026-10-18).
- Typed Rust core APIs intentionally exclude invalid JavaScript runtime input types, per the August 12, 2026 decision. Keep exclusions explicit in specs and traceability.
- Write and run a failing behavioral or differential test before production code. Differential tests run the pinned TS module through `oracle/` via `tsx` when the module is deterministic and loadable.
- Keep one traceability doc per module: exported symbols and branches mapped to tests or explicit gaps.
- Record source bugs as discrepancies. Do not silently fix or preserve them without classification.
- Replace old paths instead of adding fallbacks.
- Pin direct dependencies exactly and document their purpose in `docs/dependencies.md`.
- Run `cargo fmt --check` before every commit.
- Slice done: focused tests and targeted clippy pass. Milestone done: `cargo check --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, readability audit, independent verification.
