# Thinking timeout recovery

AgentSession ports the frozen `core/agent-session.ts` thinking watchdog and per-dispatch recovery. [Traceability](../porting/traceability/agent-session.md#thinking-timeout-dispatch) records proof and remaining integration gaps.

## What it must do

- [x] Use a twenty-minute default; observers and nonpositive configured durations skip the deadline, tools suspend it, and errors distinguish main/child sessions.
- [x] Permit one zero-delay recovery per prompt/continuation, remove the trailing interrupted assistant, and preserve original user input without duplication.
- [x] Keep the public dispatch pending through recovery; reject the second timeout and leave subsequent responses for a new explicit prompt.
- [x] Reset the recovery allowance for each explicit prompt or continuation.
- [x] Manual abort prevents recovery; abortRetry cancels an active recovery, reporting `Retry cancelled` through auto_retry_end.
- [ ] Match exact frozen RPC events/messages including faux input/cacheRead allocation after abort.
- [ ] Share the allowance with post-run compaction/queued continuation, including cancellation while waiting for the turn-start lock.
- [x] Schedule literal idle compaction at dispatch finalization; [idle tests](idle-compaction.md) exercise actual compaction after the due time.

## How it works

- [AgentSession traceability](../porting/traceability/agent-session.md#thinking-timeout-dispatch).

## Implementation inventory

- `crates/pi-coding-agent/src/core/agent_session.rs`: watchdog, dispatch ownership, recovery, retry reporting and cancellation.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/agent_session.rs`: thinking timeout lifecycle, tool suspension, observer exemption, main/child subject, reset and cancellation.
- `crates/pi-coding-agent/tests/differential_rpc_mode.rs::thinking_timeout_recovery_matches_frozen_rpc`: actual source process, full event/message equality; currently RED on faux cache ordering.

## Known gaps (current cycle)

- [ ] Frozen pending faux factory resolves on abort and warms its prompt cache before recovery; native spawned factory runs later. Preserve the RED rather than mask usage fields; parent/user decision on confined continuation semantics pending.
- [ ] Restriction validation cancellation/finalization belongs to the later restriction execution integration.

## Out of scope

- Live-provider testing; all proof uses owned isolated faux processes.
