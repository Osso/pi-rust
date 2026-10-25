# Agent-session management lifecycle

Frozen authority: `agent-session.ts:5933–5994` at `4f9e6f8a4`.

- [x] Naming and clearing emit the existing session-info event, then start extension listener effects before returning.
- [x] Provider-native summary materialization clears its running controller after authentication or settings errors, as well as after generation. A failed attempt must not prevent retry.

Implementation: the existing inline-poll event dispatch is shared with thinking events; only pending work is spawned. Materialization authentication and settings resolution remain inside its existing finalizer scope. No new carrier, dependency, persistence format or public API is introduced.

Proof: two native RED/GREEN tests in `agent_session_management_lifecycle.rs`; existing thinking-prefix and real-effort differential tests also pass after dispatch reuse. See the [proof ledger](../porting/agent-session-repin-proof.md).
