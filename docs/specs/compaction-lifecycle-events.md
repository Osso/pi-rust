# Compaction lifecycle events

Agent-session compaction exposes the frozen saved-entry event and controller lifetime. The [module trace](../porting/traceability/agent-session.md) records source branches and proof.

## What it must do

- [x] Await `session_compact` after saving/rebuilding context, before `compaction_end` and operation settlement.
- [x] Expose the saved entry, extension origin, reason and retry flag.
- [x] Preserve the tagged compaction entry in serialized extension events and accept/re-emit frozen event records.
- [x] Keep auto-compaction active through its completion event; keep manual cancellation active through its error event. Successful manual compaction clears its controller before completion.
- [x] Clear compaction state after completion or cancellation.

### Manual tool continuation

- [ ] Manual compaction of an active tool turn reports retry and resumes after aborting/waiting for that tool, as frozen.

## How it works

- [Agent-session trace](../porting/traceability/agent-session.md)

## Implementation inventory

- `crates/pi-coding-agent-core/src/core/agent_session.rs`: persistence, saved-entry emission and controller cleanup.
- `crates/pi-coding-agent-tools/src/core/extensions/types.rs` and `runner.rs`: existing saved-event carrier/dispatch.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/agent_session_compaction_events.rs`
- `crates/pi-coding-agent/tests/agent_session_manual_tool_compaction.rs`
- `oracle/agent-session-compaction-events.ts`: real frozen hooks with a settlement gate.

## Known gaps (current cycle)

The four lifecycle scenarios pass. Active-tool manual continuation has RED proof; GREEN is pending. Duplicate-summary selection follows the audited source lookup but is not in this runtime matrix.

## Out of scope

The frozen first-matching-summary lookup is preserved, including selecting an older entry for duplicate summaries. No source bug is silently corrected.
