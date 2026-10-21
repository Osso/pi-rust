# Agent core API

Reference: immutable `4f9e6f8a4`, `packages/agent-core/src/{agent-loop,agent,types,proxy,node,index}.ts`.

- Restored tool batches exclude completed calls and never replay an assistant stopped with `error` or `aborted`; expose `restoredPendingToolMessage` as `restored_pending_tool_message`.
- A run failure preserves the current assistant partial, including content/usage/identity; emit `message_start` only when no assistant partial exists.
- Reset clears transcript, queues and runtime state even during an active run; it does not abort or release that run.
- Proxy `toolcall_end` carries only `contentIndex`; finalize the tool call reconstructed from start/delta events, ignoring extra end-event fields.

Scope excludes shared content/details losslessness. Other old-only harness/session/telemetry/tools/search exports await their owning slices. Proof status and remaining differences: [slice-1 traceability](../porting/traceability/agent-core-api-repin.md).
