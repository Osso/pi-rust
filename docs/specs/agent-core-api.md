# Agent core API

Reference: immutable `4f9e6f8a4`, `packages/agent-core/src/{agent-loop,agent,types,proxy,node,index}.ts`.

- Restored tool batches exclude completed calls and never replay an assistant stopped with `error` or `aborted`; expose `restoredPendingToolMessage` as `restored_pending_tool_message`.
- A run failure preserves the current assistant partial, including content/usage/identity; emit `message_start` only when no assistant partial exists.
- Reset clears transcript, queues and runtime state even during an active run; it does not abort or release that run.
- Rejected low-level parallel sinks do not cancel already-started sibling tool promises. Any rejected update sink rejects the batch immediately, even while another sink stays pending.
- Steering messages clear the pending end-turn reminder before the next model request.
- Low-level steering/follow-up queue callbacks may reject. Propagate the original error at the queue poll, without fabricating assistant or agent-end events; harness queue drains restore removed messages when their queue-update hook rejects.
- Aborting a run does not cancel a tool future that ignores the signal. It continues on Tokio until completion or runtime shutdown; late results and updates are ignored. Task panics are consumed and logged by an observer after abort.
- Proxy request options use the frozen whitelist; omit old-only sampling parameters.
- Proxy cancellation interrupts a pending body read; empty error text or a failing non-2xx body read keeps the status-based error message.
- Empty assistant error text does not populate state.errorMessage. Null after-hook details retain executed details under the source's nullish-coalescing rule; the Option<Value> representation is unchanged.
- After-tool overrides rebuild the result payload without retaining the original `isError` hint; the finalized error flag remains separate.
- Stop hooks receive turn context only, not an additional abort signal.
- Proxy `toolcall_end` carries only `contentIndex`; finalize the tool call reconstructed from start/delta events, ignoring extra end-event fields.

- Agent constructor exposes onPayload/onResponse/onRetry rather than an unrestricted stream-options bag. Mutable provider functions, callbacks, budgets and retry cap use Rust setters; each run snapshots provider configuration. Existing prepareNextTurn wrappers read the current hook after turn listeners, preserving context-hook precedence.
- Agent listeners use live set iteration and deduplicate identical listeners. Await each listener; log rejection and continue without failing the run. Low-level event sinks still propagate rejection.
- The default stream uses `pi-ai/compat.streamSimple`; no process-global stream registration API exists.
- Stop hooks observe context replaced by `prepareNextTurn`, including restored tool batches.
- Argument preparation affects validated execution arguments, not the raw tool call passed to hooks.
- Execution metadata preparation uses the last tool definition for duplicate names; execution lookup retains first-match behavior.
- Before-tool results expose only block/reason. Blocking never terminates a batch; an empty reason uses the default blocked message.
- Agent tool results expose content/details/isError/terminate only. No usage or added-tool-name metadata injection; coding-agent wrappers return the tool's value without active-tool diffing.

Scope excludes shared content/details losslessness. Other old-only harness/session/telemetry/tools/search exports await their owning slices. Proof status and remaining differences: [slice-1 traceability](../porting/traceability/agent-core-api-repin.md).
