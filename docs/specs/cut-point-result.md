# Cut-point result

This specification covers the module-local `CutPointResult` value interface from `packages/agent/src/harness/compaction/compaction.ts:450-457`. It defines the Rust value boundary; cut-point selection and compaction orchestration remain deferred.

## What it must do

- [x] Expose a public mutable `CutPointResult` through `pi_agent_core::harness::compaction`.
- [x] Preserve required fields in source order: `first_kept_entry_index: f64`, `turn_start_index: f64`, and `is_split_turn: bool`.
- [x] Derive `Debug` only without constructors, validation, normalization, serialization, or integer narrowing.
- [x] Preserve ordinary values, the `-1` sentinel, signed zero, subnormals, finite extremes, infinities, and distinct NaN payloads in both numeric fields.
- [ ] Compare source-shaped records with the pinned TypeScript interface through exactly eight differential request groups `13130..=13137` and direct-module type proof.

## How it works

- [Cut-point result traceability](../porting/traceability/cut-point-result.md)
- [Compaction compatibility matrix](compatibility.md)

## Implementation inventory

- `crates/pi-agent-core/src/harness/compaction/mod.rs` — module-local value record.
- `crates/pi-agent-core/tests/cut_point_result.rs` — focused field, mutation, Debug, and binary64 proof.
- `run-tests.sh` — focused test registration.

## Tests asserting this spec

- `crates/pi-agent-core/tests/cut_point_result.rs`

## Known gaps (current cycle)

- [ ] Add eight direct source-shaped differential groups `13130..=13137` and direct-module type proof.
- [ ] Complete final workspace, static, readability, documentation, clean-pin, and inherited verification gates.

## Out of scope

`findCutPoint`, `findTurnStartIndex`, entry/session/message unions, cut-point discovery, protected-suffix/token logic, compaction preparation/orchestration, constructors, serialization, validation, normalization, integer narrowing, package or harness-root exports, the coding-agent duplicate, dependencies, and unrelated compaction APIs remain deferred.
