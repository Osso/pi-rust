# Agent lifecycle inspection

Frozen `extensions/agent-viewer/src/runtime.ts` and `core/agent-lifecycle-trace.ts` at `4f9e6f8a4` govern historical agent inspection.

## What it must do

- [x] Require storeSessionId for trace; preserve missing-agent and transcript-session mismatch errors.
- [x] Read root/descendant snapshots, ownership, parent start/complete records, successful child end_turn records and terminal outbox without mutation.
- [x] Return exact frozen trace JSON and human-readable text, including timestamp origin and same-timestamp kind ordering.
- [x] Exclude unrelated agents and failed end_turn results.
- [ ] Exercise every malformed/missing transcript and ownership branch against frozen.

## How it works

- [Trace symbol mapping and proof gaps](../porting/traceability/agent-lifecycle-trace.md)

## Implementation inventory

- `crates/pi-coding-agent/src/core/agent_lifecycle_trace.rs` — whole reader and formatter.
- `crates/pi-coding-agent/src/bundled_extensions/agents_core/viewer.rs` — persisted viewer integration and trace validation.

## Tests asserting this spec

- `tests/differential_agent_lifecycle_trace.rs` — exact four viewer requests against frozen, same isolated DB/files.

## Known gaps (current cycle)

- [ ] Additional malformed/cycle/absent data branch proof, listed in traceability.

## Out of scope

- Repairs/reconciliation of historical data. Inspection reports source behavior, not invented recovery.
