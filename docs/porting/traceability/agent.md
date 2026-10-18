# Traceability: `packages/agent/src/agent.ts`

Rust: `crates/pi-agent-core/src/agent/agent.rs`.
Tests: `crates/pi-agent-core/tests/agent.rs`, `agent_abort.rs`, `agent_continue_queue.rs`,
`differential_agent_loop/mod.rs` (`agent_text`, `agent_continue_queue`, `abort_tool`).

## Exports

| TypeScript export | Rust item | Tests |
|---|---|---|
| `QueueMode` re-export | `harness::types::QueueMode` | `queue_mode.rs` |
| `AgentOptions` | `agent::AgentOptions` (+ `AgentInitialState`) | default/custom state tests |
| `Agent` | `agent::Agent` (`Arc` handle, `Clone`) | all agent tests |

## Members

| TS member | Rust item | Tests |
|---|---|---|
| `constructor(options)` (throws without a default stream fn) | `Agent::new -> Result` | `uses_configured_default_when_stream_fn_is_omitted`; missing-default error covered in `stream_fn.rs` |
| `subscribe(listener) -> unsubscribe` | `subscribe(AgentListener) -> u64`, `unsubscribe(id)`; listeners return `Result<(), AgentError>` | `subscribes_to_events_and_unsubscribes` |
| `state` (accessor properties, copies on assignment) | `state()` snapshot + typed getters/setters | `updates_state_with_mutators` |
| `steeringMode` / `followUpMode` | `steering_mode`/`set_steering_mode`, `follow_up_mode`/`set_follow_up_mode` | code inspection; drain behavior tested via `continue_keeps_one_at_a_time_*` |
| `steer(message)` (aborts active model request) | `steer` | `aborts_active_model_request_when_steering_is_queued` |
| `followUp(message)` | `follow_up` | `supports_steering_and_follow_up_queues` |
| `clearSteeringQueue` / `clearFollowUpQueue` / `clearAllQueues` | same | `supports_steering_and_follow_up_queues` |
| `hasQueuedMessages` | `has_queued_messages` | same |
| `signal` | `signal() -> Option<CancellationToken>` | `passes_active_abort_signal_to_subscribers` |
| `abort` | `abort` | `abort_is_a_noop_without_an_active_run`, abort tests |
| `waitForIdle` | `wait_for_idle` | `wait_for_idle_waits_for_async_subscribers` |
| `reset` (throws while processing) | `reset() -> Result` | `rejects_reset_while_processing_without_corrupting_transcript` |
| `prompt(input, images?)` | `prompt` / `prompt_with_images` | `prompt_accepts_agent_messages_and_batches` |
| `continue(options)` | `continue_run(ContinueOptions)` | continue tests |
| `normalizePromptInput` | `normalize_prompt_input` | prompt tests |
| `runPromptMessages` / `runContinuation` | private equivalents | all run tests |
| `createContextSnapshot` | same | code inspection |
| `createLoopConfig` (reasoning off→undefined, sessionId, hooks, queue pollers, skip initial steering poll) | same | `forwards_session_id_to_stream_function_options`, `keeps_legacy_prepare_next_turn_signal_callback_behavior`, `forwards_should_stop_after_turn_through_agent_options`, `continue_keeps_one_at_a_time_*` |
| `runWithLifecycle` (single active run, state flags, failure handling, finish) | `begin_run` / `finish_run_with` / `finish_run` | `rejects_prompt_while_streaming`, `rejects_continue_while_streaming`, failure test |
| `handleRunFailure` (synthetic assistant message, full event sequence) | `build_failure_message` + event emission | `emits_full_lifecycle_events_for_run_failures` |
| `finishRun` | `finish_run` | `wait_for_idle_*`, reset test |
| `processEvents` state reduction + awaited listeners | `process_events` (listener errors propagate) | lifecycle/state tests; differential `listener_*_agent` |
| `PendingMessageQueue` `all` / `one-at-a-time` | same | continue steering test, queue mode tests |

## Branches

- `prompt`/`continue`/`reset` re-entrancy errors use the exact TS messages (`AgentError` constructors).
- `continue` drains steering first (skipping the initial steering poll for that run), then follow-ups, then falls back to a continuation.
- `steer` aborts only while `isModelRequestActive` is true.
- `agent_end` listeners are awaited before `isStreaming` becomes false.
- Failure path emits `message_start`/`message_end`/`turn_end`/`agent_end` for a synthetic assistant message and sets `state.errorMessage` from `turn_end`.
- A rejecting listener rejects the loop's `emit`; `runWithLifecycle` then runs the failure sequence. If the listener rejects again inside that sequence, `prompt`/`continue` rejects with the listener error and the remaining failure events are not emitted, matching TS.

## Discrepancies / exclusions

- State accessor properties become explicit getters/setters; getters return copies. TS `state.tools.push(...)` mutation is not preserved (assign a new vector instead).
- `subscribe` returns an id instead of an unsubscribe closure (`unsubscribe(id)`).
- `prepareNextTurnWithContext` and legacy `prepareNextTurn(signal)` are separate option fields, mirroring TS.
- `Agent` is `Clone` (shared `Arc` state) as a Rust handle adaptation; TS instances are not cloneable.
