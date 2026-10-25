# Idle compaction

Preserve frozen `core/idle-compaction.ts` and AgentSession's idle prompt-cache compaction lifecycle. [Traceability](../porting/traceability/idle-compaction.md) maps source and proof.

## What it must do

- [x] Compute due time at 90% of the API cache lifetime: claude-bridge60 minutes, openai-codex-responses30 minutes; require at least100000 context tokens calculated from usage components, not totalTokens.
- [x] Preserve numeric timestamps; unsupported API keys return no deadline. Inherited-key NaN behavior is an approved runtime-oddity gap (2026-10-23).
- [x] Schedule after dispatch finalization; compact once at the due time with reason `idle`, without retrying the turn.
- [x] Recheck settings, latest-message reference, model, queues, disposal and manual-compaction boundary before execution. Equal-valued replacement does not retain the scheduled reference.
- [x] Schedule restored contexts at session-start/reload and skip observers/errors; aborted messages remain eligible, matching the source's error-only exclusion.
- [x] Defer across busy tools and replace the deadline from the latest request timestamp.

## How it works

- [Identity and scheduler traceability](../porting/traceability/idle-compaction.md).

## Implementation inventory

- `crates/pi-coding-agent-tools/src/core/idle_compaction.rs`: pure deadline calculation.
- `crates/pi-coding-agent-core/src/core/agent_session.rs`: target selection, owned timer, lock/rechecks and auto compaction.
- `crates/pi-agent-core/src/agent/agent.rs`: atomic latest-message/reference observation.
- `crates/pi-coding-agent-tools/src/core/extensions/types.rs`: native idle reason tag.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/differential_idle_compaction.rs`: actual frozen24 explicit API/token/timestamp cases; prototype emulation cases retired.
- `crates/pi-coding-agent/tests/agent_session.rs`: ordinary idle lifecycle tests, real compaction/state changes with paused timers and owned cleanup.

## Known gaps (current cycle)

- [ ] Restored-context proof reconstructs SessionManager/Agent state in-process; a crash/restart process-boundary test remains open.
- [ ] Targeted clippy/readability and independent final gate remain open at lane level.

## Out of scope

- Real provider caches; tests use faux responses without network calls.
- Reproducing the frozen TTL-table prototype lookup discrepancy; native uses own-key lookup.
