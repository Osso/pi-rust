# AgentHarness

Reference: immutable `4f9e6f8a4`, `packages/agent-core/src/harness/{agent-harness,types}.ts`.

- Run real turns through the agent loop and native Models collection. Persist message-end before subscribers; flush deferred mutations after turn-end subscribers, emit save_point, then set idle before agent_end and settled.
- Snapshot session context, resources, active tools, model/thinking level and curated stream options per turn; refresh after each turn. Await provider-request patches in registration order. Preserve native header values and arbitrary metadata; reject non-scalar header names explicitly at the AI boundary.
- Catch loop/acquisition failures and report an assistant with exactly one empty text block, zero usage, active model identity, original UTF-16 error and error/aborted stop reason. Failure-report rejection yields the frozen aggregate message and both original typed causes, not message-only wrappers.
- Retain next-turn queues on failed queue-update drains. Steering/follow-up drain one or all according to mode; abort clears those two queues but not next-turn messages and waits for the active run.
- Preserve frozen explicit compaction/tree hooks, cancellation, append/move ordering, editor text, settings validation and error classification. Returned SessionErrors keep session classification; rejected native errors use the active operation fallback (compaction or branch_summary during navigation persistence). Payload/response callbacks preserve branded built-in errors and exact name/code/cause through the shared rejection carrier, while retaining the original typed Rust source. Rust-native extension error types without JS fields project only as plain Error(Display), with no invented code/cause. Delete old-only scaffold/lane APIs.

Whole-module map, proof levels and approved exclusions: [traceability](../porting/traceability/agent-core-agent-harness.md).
