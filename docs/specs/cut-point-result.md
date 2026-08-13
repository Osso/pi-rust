# Cut-point result

This specification covers the module-local `CutPointResult` value interface from `packages/agent/src/harness/compaction/compaction.ts:450-457`. It defines the Rust value boundary; cut-point selection and compaction orchestration remain deferred.

## What it must do

- [x] Expose a public mutable `CutPointResult` through `pi_agent_core::harness::compaction`.
- [x] Preserve required fields in source order: `first_kept_entry_index: f64`, `turn_start_index: f64`, and `is_split_turn: bool`.
- [x] Derive `Debug` only without constructors, validation, normalization, serialization, or integer narrowing.
- [x] Preserve ordinary values, the `-1` sentinel, signed zero, subnormals, finite extremes, infinities, and distinct NaN payloads in both numeric fields.
- [x] Compare source-shaped final records with the pinned TypeScript interface through exactly eight differential request groups `13130..=13137`; prove mutability separately with direct-module type proof.

## How it works

- [Cut-point result traceability](../porting/traceability/cut-point-result.md)
- [Compaction compatibility matrix](compatibility.md)

## Implementation inventory

- `crates/pi-agent-core/src/harness/compaction/mod.rs` — module-local value record.
- `crates/pi-agent-core/tests/cut_point_result.rs` — focused field, mutation, Debug, and binary64 proof.
- `crates/pi-agent-core/tests/differential_truncate/cut_point_result.rs` — eight source-shaped differential request groups with binary64 normalization.
- `oracle/truncate-oracle.ts` — source-shaped CutPointResult request adapter and normalization.
- `oracle/cut-point-result-typecheck.mjs` — pinned direct-module exact-shape and mutability proof.
- `run-tests.sh` — focused and direct-module typecheck registration.

## Tests asserting this spec

- `crates/pi-agent-core/tests/cut_point_result.rs`
- `crates/pi-agent-core/tests/differential_truncate/cut_point_result.rs`
- `oracle/cut-point-result-typecheck.mjs`

## Known gaps (current cycle)

- [ ] Complete final workspace, static, readability, documentation, clean-pin, and inherited verification gates.

## Out of scope

`findCutPoint`, `findTurnStartIndex`, entry/session/message unions, cut-point discovery, protected-suffix/token logic, compaction preparation/orchestration, constructors, serialization, validation, normalization, integer narrowing, package or harness-root exports, the coding-agent duplicate, dependencies, and unrelated compaction APIs remain deferred. Differential cases use final source-shaped records; direct type proof separately exercises public mutation without claiming runtime construction or validation.
