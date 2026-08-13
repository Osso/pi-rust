# Queue mode

This specification covers the typed `QueueMode` union from `packages/agent/src/types.ts:44-50` and its public package export through `packages/agent/src/index.ts:145`. Queue draining behavior remains deferred.

## What it must do

- [x] Expose a closed public enum with exactly `All` and `OneAtATime` variants.
- [x] Derive exactly `Debug`, `Clone`, `Copy`, `PartialEq`, and `Eq`.
- [x] Return the exact source literals `"all"` and `"one-at-a-time"` from `as_str`.
- [x] Re-export `QueueMode` from `pi_agent_core::harness` without a crate-root direct re-export.
- [x] Cover both variants, literals, equality, copying, Debug output, and the harness-root import in focused behavioral tests.

## How it works

- [Queue mode traceability](../porting/traceability/queue-mode.md)

## Implementation inventory

- `crates/pi-agent-core/src/harness/types.rs` — closed enum and literal mapping.
- `crates/pi-agent-core/src/harness/mod.rs` — harness-root re-export.
- `crates/pi-agent-core/tests/queue_mode.rs` — focused behavioral/API proof.
- `run-tests.sh` — focused test registration.

## Tests asserting this spec

- `crates/pi-agent-core/tests/queue_mode.rs`

## Known gaps (current cycle)

None for the focused production/value phase.

## Out of scope

Queue draining, parsing, `FromStr`, `Display`, serde, validation, `ToolExecutionMode`, dependencies, differential/oracle fixtures, and adjacent queue APIs remain deferred.
