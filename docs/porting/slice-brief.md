# Slice brief (for porting agents)

Read `AGENTS.md` first. Use absolute paths and `.cwd(<worktree>)`. Read-only source: use `worktree` and full revision from [source-lock.json](source-lock.json), currently 4f9e6f8a4. Old approvals/proofs are pending revalidation; see [re-pin boundary](repin-infra.md).

## Workflow per module
1. Read the TS module and its tests (`packages/<pkg>/test/*`) to learn observable behavior.
2. RED: write Rust tests in `crates/<crate>/tests/<module>.rs` asserting concrete behavior (port relevant TS test cases). Run them; confirm they fail for the missing API.
3. Implement the module in `crates/<crate>/src/...` mirroring the TS file layout (kebab-case file → snake_case module).
4. Differential test when the module is deterministic and importable under `tsx`: add `oracle/<module>-oracle.ts` (JSON-lines request/response over stdin/stdout; see `oracle/skill-block-oracle.ts`) and `crates/<crate>/tests/differential_<module>.rs` using the `TsOracle` pattern in `crates/pi-coding-agent/tests/support/oracle.rs` (copy the support file into the crate's `tests/support/` if missing). Skip when the module needs network/TTY/OS state; say so in traceability.
5. Write `docs/porting/traceability/<module>.md`: each exported symbol / branch → Rust item + test, or explicit gap with reason. Record source bugs as discrepancies.
6. Update the rows in `docs/porting/coverage.md` (Status `done`/`partial`, Rust path, notes).
7. New deps: exact pin (`=x.y.z`) in the crate's Cargo.toml + purpose line in `docs/dependencies.md`. Prefer std; reuse deps already in the workspace.
8. Gates before each commit: `cargo fmt`, `cargo test -p <crate>` (targeted tests), `cargo clippy -p <crate> --all-targets -- -D warnings`. Never suppress warnings with `allow`.
9. Commit in your worktree on your branch with a brief technical message. Do not push, merge, or touch other worktrees.

## Rust conventions
- Edition 2024, `unsafe_code = "forbid"`. Idiomatic Rust: enums for TS unions, `Option` for optional fields, `Result` with typed errors for throws.
- JSON-shaped records: `serde` derive with `#[serde(rename_all = "camelCase")]` matching TS wire names.
- Async: `tokio`; cancellation: `tokio_util::sync::CancellationToken` (maps `AbortSignal`).
- Keep functions small, nesting ≤ 2, no speculative abstractions, no fallbacks.

## Report back (final message)
Branch + commit hashes, modules ported, tests added (counts, pass/fail), differential coverage, gaps, discrepancies, new deps.

- Test processes run through `scripts/cargo-runner.sh` (cargo `runner`), which gives each test binary a private `PI_CODING_AGENT_STATE_DIR` like TS `test/global-setup.ts`. Tests must never resolve the real `~/.local/state/pi` or `~/.config/pi/agent`; pass temp `agent_dir`/`control_db_path` explicitly where the TS test does.
