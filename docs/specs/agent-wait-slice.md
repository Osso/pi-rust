# Foreground agent wait slice

Frozen `extensions/agents-core/src/runtime.ts:196,2126-2295,3157-3168` bounds foreground wait_agent calls from the latest model request start.

## What it must do

- [x] Track model_request_start for supervisor runtimes; clear the timestamp on agent_end and session_start. Child runtimes do not overwrite the supervisor timestamp.
- [x] Finish the wait after 25 minutes from the latest model request start, not after 25 minutes from the tool call.
- [x] Return timedOut true and explicit no-cancellation text; keep active agents running.
- [x] Leave idle waits unbounded by the foreground deadline after agent_end.
- [ ] Prefer terminal/coordination input over a simultaneous deadline wake.

## How it works

- [Agents-core traceability](../porting/traceability/agents-core.md)

## Implementation inventory

- `bundled_extensions/agents_core/runtime.rs` — shared latest request timestamp.
- `bundled_extensions/agents_core/registration.rs` — supervisor model/session/agent hooks.
- `bundled_extensions/agents_core/wait_runtime.rs` — captured deadline, timer and poll checks.
- `core/extensions/types.rs`, `runner.rs` — model request start/end event API and dispatch.
- `core/agent_session.rs` — forwards real model request events to extension runner.

Paths relative to `crates/pi-coding-agent/src/`.

## Tests asserting this spec

- `tests/agents_core_runtime.rs::wait_slice_uses_latest_model_request_and_does_not_cancel_active_agents` — real SQLite active child, extension event dispatch, paused Tokio deadline, agent_end reset.

## Known gaps (current cycle)

- [ ] Deadline/terminal coordination tie and real AgentSession model-event forwarding need separate integration assertions.

## Out of scope

- Cancellation settlement changes and whole agents-runtime parity.
