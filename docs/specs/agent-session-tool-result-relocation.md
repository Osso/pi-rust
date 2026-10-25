# Agent-session tool-result relocation

Frozen authority: `agent-session.ts:1550–1593,1640–1680` at `4f9e6f8a4`.

- [x] Persist a completed tool result before delivering its scheduled relocation.
- [x] Await relocation preparation before the agent can emit its end event or request another model response.
- [x] Report the prepared cwd-relocation continuation and activate it after the end event.

Use the existing runner relocation API and callback-error carrier. No new carrier, dependency, public API or JSONL codec is introduced.

Proof: a native real-tool test schedules relocation from its execution context. Its yielding prepare callback observes the persisted result; the test asserts prepare → relocation end event → activation and exactly one provider call. Before the fix, only a non-relocation end event occurred. See the [proof ledger](../porting/agent-session-repin-proof.md).
