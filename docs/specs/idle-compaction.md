# Idle compaction

Preserve frozen `core/idle-compaction.ts` and AgentSession's idle prompt-cache compaction lifecycle. [Traceability](../porting/traceability/idle-compaction.md) maps source and proof.

## What it must do

- [x] Compute due time at 90% of the API cache lifetime: claude-bridge60 minutes, openai-codex-responses30 minutes; require at least100000 context tokens calculated from usage components, not totalTokens.
- [x] Preserve numeric timestamps and the frozen inherited-key discrepancy: Object.prototype names produce NaN above the token minimum, not absence. Node timer coercion schedules NaN at1ms.
- [x] Schedule after dispatch finalization; compact once at the due time with reason `idle`, without retrying the turn.
- [x] Recheck settings, latest-message reference, model, queues, disposal and manual-compaction boundary before execution. Equal-valued replacement does not retain the scheduled reference.
- [ ] Schedule restored contexts at session-start/reload and skip observers, with dedicated runtime proof.
- [ ] Prove busy-tool deferral and latest-request deadline replacement at the full runtime boundary.

## How it works

- [Identity and scheduler traceability](../porting/traceability/idle-compaction.md).

## Implementation inventory

- `crates/pi-coding-agent/src/core/idle_compaction.rs`: pure deadline calculation.
- `crates/pi-coding-agent/src/core/agent_session.rs`: target selection, owned timer, lock/rechecks and auto compaction.
- `crates/pi-agent-core/src/agent/agent.rs`: atomic latest-message/reference observation.
- `crates/pi-coding-agent/src/core/extensions/types.rs`: native idle reason tag.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/differential_idle_compaction.rs`: actual frozen96 API/token/timestamp cases, including all source Object.prototype names and distinct undefined/NaN encoding.
- `crates/pi-coding-agent/tests/agent_session.rs`: four idle tests, real compaction/state changes with paused timers and owned cleanup.

## Known gaps (current cycle)

- [ ] Source scenario coverage still needs restored session/reload, observer, busy tool and deadline refresh proof.
- [ ] Targeted clippy/readability and independent final gate remain open at lane level.

## Out of scope

- Real provider caches; tests use faux responses without network calls.
- Fixing the frozen TTL-table prototype lookup discrepancy.
