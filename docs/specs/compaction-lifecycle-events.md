# Compaction lifecycle events

Agent-session compaction exposes the frozen saved-entry event and controller lifetime. The [module trace](../porting/traceability/agent-session.md) records source branches and proof.

## What it must do

- [ ] Await `session_compact` after saving/rebuilding context, before `compaction_end` and operation settlement.
- [ ] Expose the saved entry, extension origin, reason and retry flag.
- [ ] Preserve the tagged compaction entry in serialized extension events and accept/re-emit frozen event records.
- [ ] Keep auto-compaction active through its completion event; keep manual cancellation active through its error event. Successful manual compaction clears its controller before completion.
- [ ] Clear only the matching auto controller after completion or cancellation.

## How it works

- [Agent-session trace](../porting/traceability/agent-session.md)

## Implementation inventory

- `crates/pi-coding-agent-core/src/core/agent_session.rs`: persistence, saved-entry emission and controller cleanup.
- `crates/pi-coding-agent-tools/src/core/extensions/types.rs` and `runner.rs`: existing saved-event carrier/dispatch.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/agent_session_compaction_events.rs`
- `oracle/agent-session-compaction-events.ts`: real frozen hooks with a settlement gate.

## Known gaps (current cycle)

- [ ] Runtime GREEN is pending; four corrected source/native cases currently provide RED evidence.

## Out of scope

The frozen first-matching-summary lookup is preserved, including selecting an older entry for duplicate summaries. No source bug is silently corrected.
