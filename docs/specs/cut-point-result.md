# Cut-point result

This specification covers the module-local TypeScript `CutPointResult` interface from `packages/agent/src/harness/compaction/compaction.ts:450-457` and its Rust value boundary. Cut-point selection and compaction orchestration remain deferred.

## What it must do

- [x] Expose a public mutable `CutPointResult` through `pi_agent_core::harness::compaction`.
- [x] Preserve required fields in source order: `first_kept_entry_index: f64`, `turn_start_index: f64`, and `is_split_turn: bool`.
- [x] Derive `Debug` only without constructors, validation, normalization, serialization, or integer narrowing.
- [x] Preserve ordinary values, the `-1` sentinel, signed zero, subnormals, finite extremes, infinities, and distinct NaN payloads in both numeric fields.
- [x] Compare source-shaped plain records through exactly eight differential request groups `13130..=13137`; prove the pinned module interface's exact shape and mutability separately with standalone direct-module type proof.

## How it works

- [Cut-point result traceability](../porting/traceability/cut-point-result.md)
- [Compaction compatibility matrix](compatibility.md)

## Implementation inventory

- `crates/pi-agent-core/src/harness/compaction/mod.rs` — module-local value record.
- `crates/pi-agent-core/tests/cut_point_result.rs` — focused field, mutation, Debug, and binary64 proof.
- `crates/pi-agent-core/tests/differential_truncate/cut_point_result.rs` — eight source-shaped differential request groups with binary64 normalization.
- `oracle/truncate-oracle.ts` — source-shaped plain-record request adapter and normalization.
- `oracle/cut-point-result-typecheck.mjs` — standalone pinned direct-module `import type` exact-shape and mutability proof.
- `run-tests.sh` — focused test and standalone direct-module typeproof registration.

## Tests asserting this spec

- `crates/pi-agent-core/tests/cut_point_result.rs`
- `crates/pi-agent-core/tests/differential_truncate/cut_point_result.rs`
- `oracle/cut-point-result-typecheck.mjs`

## Known gaps (current cycle)

- [x] Complete final workspace/static/readability/documentation/clean-pin gates: workspace `cargo test --workspace --all-targets` captured 241 passed, 0 failed, 2 ignored in `/tmp/pi-rust-cut-point-result-final-workspace-tests.out`; all-target check, all-feature Clippy with `-D warnings`, fmt, `git diff --check`, and Node syntax all exited 0 in the corresponding final artifacts; readability and documentation audits passed; exact source/Rust pins passed.
- [x] Complete inherited independent verification at clean Rust HEAD `b8783cb`: fresh focused 3 passed/0 failed, differential 1 passed/0 failed, oracle/fmt/diff/check/Clippy/Node exit 0, saved workspace 241 passed/0 failed/2 ignored, readability runner 25 lines and oracle functions 4 and 13 lines, documentation consistency; `OVERALL: PASS`.

## Out of scope

`findCutPoint`, `findTurnStartIndex`, entry/session/message unions, cut-point discovery, protected-suffix/token logic, compaction preparation/orchestration, constructors, serialization, validation, normalization, integer narrowing, package or harness-root exports, the coding-agent duplicate, dependencies, and unrelated compaction APIs remain deferred. Differential cases use final source-shaped records; direct type proof separately exercises public mutation without claiming runtime construction or validation.
