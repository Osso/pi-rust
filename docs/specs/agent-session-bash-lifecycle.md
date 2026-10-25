# Agent-session bash lifecycle

Frozen authority: `agent-session.ts:5807–5950` at `4f9e6f8a4`.

- [x] An empty configured shell prefix leaves the command unchanged.
- [x] Session execution tracks one controller: starting another execution replaces it, and abort targets only the current controller.
- [x] Completion clears that slot after recording the result; commit listeners still observe a running execution.

Source discrepancy classification: completing an older concurrent execution clears the newer controller as well. Preserve this observable frozen behavior, rather than the previous native controller collection. The controlled overlapping-execution test covers this ordering; it is not a scheduler-race approximation.

Proof: `agent_session::bash_parity` has three native RED/GREEN tests. Existing bash stream, abort, commit and JSONL/RPC number tests retain responsibility for output and persistence. The fix does not change the message codec or persisted fields. See the [proof ledger](../porting/agent-session-repin-proof.md).
