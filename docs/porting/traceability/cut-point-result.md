# Cut-point result traceability

Pinned source: `packages/agent/src/harness/compaction/compaction.ts` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `compaction.ts:450-457` | Public mutable TypeScript interface with required `firstKeptEntryIndex: number`, `turnStartIndex: number`, and `isSplitTurn: boolean` | Module-local `harness::compaction::CutPointResult` with public `f64`, `f64`, and `bool` fields in source order; focused Rust field initialization, mutation, Debug, and binary64 proof; eight direct source-shaped plain-record differential groups `13130..=13137` |
| `index.ts:58-75` | Package index exports `findCutPoint` but omits `type CutPointResult` | No `harness`-root or package stand-in; module-only Rust path preserves the selected source boundary |

## Rust boundary

- `CutPointResult` derives `Debug` only and remains publicly mutable.
- TypeScript `number` fields map to unrestricted Rust `f64` values; no integer or `usize` narrowing is added.
- The semantic `-1` turn-start sentinel and all representable binary64 bit patterns remain valid values.
- Invalid JavaScript runtime shapes and lone-surrogate strings remain outside the typed Rust boundary.

## Proof status

- Focused RED: captured output is in `/tmp/pi-rust-cut-point-result-red.out` and shows only the missing module-type import; the proof ledger records command exit 101.
- Focused GREEN: captured output is in `/tmp/pi-rust-cut-point-result-green.out` and shows 3 passed, 0 failed; the proof ledger records command exit 0.
- Formatting and diff checks produced captured output in `/tmp/pi-rust-cut-point-result-fmt-final.out` and `/tmp/pi-rust-cut-point-result-diff-check-final.out`; both are empty, and the proof ledger records exit 0 for each command.
- Differential RED: captured output is in `/tmp/pi-rust-cut-point-result-differential-red.out` and shows the expected non-JSON oracle response failure; the proof ledger records command exit 101 and identifies request `13130` as the unchanged-oracle unsupported operation.
- Differential GREEN: captured output is in `/tmp/pi-rust-cut-point-result-differential-green.out` and shows one persistent test with 1 passed and 0 failed; the proof ledger records command exit 0 for exactly eight plain-record groups (`13130..=13137`).
- Direct-module type proof: `/tmp/pi-rust-cut-point-result-oracle.out` is the captured output artifact and is empty; the proof ledger records `./run-tests.sh oracle` exit 0. `oracle/cut-point-result-typecheck.mjs` uses standalone direct-module `import type` to validate exact shape, required fields, public mutation, and unrestricted number assignments without public-index proof.
- Node syntax: `/tmp/pi-rust-cut-point-result-node-check.out` is the captured output artifact and is empty; the proof ledger records command exit 0.
- Readability: `/tmp/pi-rust-cut-point-result-readability-final` reports runner body 25 lines and new oracle functions at 4 and 13 lines; the existing baseline violation remains unchanged.
- Final workspace: `/tmp/pi-rust-cut-point-result-final-workspace-tests.out` captures `cargo test --workspace --all-targets` with 241 passed, 0 failed, and 2 ignored; the proof ledger records exit 0.
- Final static gates: `/tmp/pi-rust-cut-point-result-final-check.out` captures `cargo check --workspace --all-targets` exit 0; `/tmp/pi-rust-cut-point-result-final-clippy.out` captures all-feature Clippy with `-D warnings` exit 0; `/tmp/pi-rust-cut-point-result-final-fmt.out` captures fmt exit 0; `/tmp/pi-rust-cut-point-result-final-diff.out` captures `git diff --check` exit 0; `/tmp/pi-rust-cut-point-result-final-node.out` captures Node syntax exit 0.
- Documentation and clean pins: the dedicated spec, traceability, and concise compatibility edits are complete; exact source/Rust heads match the pinned revisions, the source worktree is clean, and Rust is dirty only for these expected uncommitted documentation edits.
- Inherited independent verification remains pending after the expected uncommitted documentation closure at code HEAD `b3f33f9`.

## Exclusions

`findCutPoint`, `findTurnStartIndex`, entry/session/message unions, cut-point discovery, protected-suffix/token logic, compaction preparation/orchestration, constructors, serialization, validation, normalization, integer narrowing, package or harness-root exports, the coding-agent duplicate, dependencies, and unrelated compaction APIs remain deferred. Differential requests normalize final source-shaped records; direct typeproof assignments prove mutability separately and do not claim runtime construction or validation.
