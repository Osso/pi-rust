# Agent-session thinking events

Thinking-level selection preserves the synchronous prefix of frozen `setThinkingLevel` event dispatch. See the [module trace](../porting/traceability/agent-session.md).

## What it must do

- [x] Start a thinking-level listener inline before the setter returns.
- [x] Retain and execute an asynchronous listener remainder after its first pending await.
- [x] Activating ultra must let the actual effort extension persist proactive mode, restore delegation tools and update status before the setter returns.

## How it works

- [Agent-session trace](../porting/traceability/agent-session.md)

## Implementation inventory

- `crates/pi-coding-agent-core/src/core/agent_session.rs`: initial event poll and pending dispatch.
- `crates/pi-coding-agent-extensions/src/bundled_extensions/effort/mod.rs`: unchanged consumer.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/agent_session_thinking_cycle.rs`
- `crates/pi-coding-agent/tests/agent_session_effort_prefix.rs`
- `oracle/agent-session-thinking-listener.ts` and `oracle/agent-session-effort-prefix.ts`

## Known gaps (current cycle)

No known gap in the synchronous-prefix scope. Generic sync/pending and actual effort observations pass against frozen; see the proof ledger.

## Out of scope

No executor is introduced for unsupported out-of-runtime embeddings. A pending remainder without Tokio is an explicit error, not a silent drop. Independent runtime/microtask interleavings remain under the repository gap policy.
