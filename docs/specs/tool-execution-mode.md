# ToolExecutionMode

This specification covers the closed `ToolExecutionMode` value from `packages/agent/src/types.ts:34-42` and its package export through `packages/agent/src/index.ts:145`. Tool preparation, scheduling, execution, and event ordering remain separate behavior.

## What it must do

- [x] Expose a closed public enum with exactly `Sequential` and `Parallel` variants.
- [x] Derive exactly `Debug`, `Clone`, `Copy`, `PartialEq`, and `Eq`.
- [x] Return the exact source literals `"sequential"` and `"parallel"` from `as_str`.
- [x] Re-export `ToolExecutionMode` from `pi_agent_core::harness` without a crate-root direct re-export.
- [x] Cover both variants, literals, equality, copying, Debug output, and the harness-root import in focused behavioral tests.

## How it works

- [Tool execution mode traceability](../porting/traceability/tool-execution-mode.md)

## Implementation inventory

- `crates/pi-agent-core/src/harness/types.rs` — closed enum and literal mapping.
- `crates/pi-agent-core/src/harness/mod.rs` — harness-root re-export.
- `crates/pi-agent-core/tests/tool_execution_mode.rs` — focused behavioral/API proof.
- `run-tests.sh` — focused test registration.

## Tests asserting this spec

- `crates/pi-agent-core/tests/tool_execution_mode.rs`
- `crates/pi-agent-core/tests/differential_truncate/tool_execution_mode.rs`
- `oracle/public-index-typecheck.mjs`

## Differential/oracle proof

- Request `13165` is the single persistent source-shaped plain-value group after QueueMode request `13164`; it contains both `sequential` and `parallel`.
- `/tmp/pi-rust-tool-execution-mode-differential-red.out` records the valid unsupported-operation RED before oracle support.
- `/tmp/pi-rust-tool-execution-mode-differential-green.out` records the differential GREEN; `/tmp/pi-rust-tool-execution-mode-oracle.out` records the pinned oracle and public-index typeproof GREEN.
- Runtime plain-value comparison and erased public-index typeproof remain separate; no constructor, parser, validation, scheduling, execution, or event-order behavior is claimed.

## Known gaps (current cycle)

- [x] One source-shaped differential/oracle group and strict pinned public-index typeproof cover both valid literals.
- [ ] Broader integration verification remains deferred to a later bounded phase.

## Out of scope

- Parsing, `FromStr`, `Display`, serde, validation, dependencies, tool scheduling or execution, event ordering, `QueueMode`, constructors, and adjacent APIs.
