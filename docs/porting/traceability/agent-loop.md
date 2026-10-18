# Traceability: `packages/agent/src/agent-loop.ts`

Rust: `crates/pi-agent-core/src/agent/agent_loop.rs`.
Tests: `crates/pi-agent-core/tests/agent_loop.rs`, `crates/pi-agent-core/tests/agent_abort.rs`,
`crates/pi-agent-core/tests/differential_agent_loop/mod.rs`.

## Exports

| TypeScript export | Rust item | Tests |
|---|---|---|
| `AgentEventSink` | `agent::AgentEventSink` | all loop tests |
| `agentLoop` | `agent::agent_loop` | `emits_events_with_agent_message_types`, differential `text`/`tools` |
| `agentLoopContinue` | `agent::agent_loop_continue` (returns `Result`) | `continue_throws_when_context_has_no_messages`, `continue_*` tests |
| `runAgentLoop` | `agent::run_agent_loop` | exercised through `agent_loop` and Agent |
| `runAgentLoopContinue` | `agent::run_agent_loop_continue` | exercised through `agent_loop_continue` |

## Branches

| TS branch | Rust handling | Tests |
|---|---|---|
| `agentLoopContinue` empty context throws | `Err(AgentError::cannot_continue_empty_context)` | `continue_throws_when_context_has_no_messages` |
| `runAgentLoop`: prompt append, `agent_start`/`turn_start`/prompt message events | `run_agent_loop` | `emits_events_with_agent_message_types` |
| `restoredPendingToolMessage` scans backwards, skips completed results, filters pending calls | `restored_pending_tool_message` | `restarts_unfinished_tool_call_before_requesting_the_model`, `restarts_only_unfinished_tool_calls_when_earlier_result_exists`, differential `continue_*` |
| continuation restores tool batch before the model request | `run_agent_loop_continue` | same |
| continuation early exit on `terminate` | same | differential `terminate_all` via loop; continuation path covered by code inspection |
| `prepareNextTurn` / `shouldStopAfterTurn` in continuation | same | differential `prepare_next_turn`/`should_stop` (prompt path) |
| main loop turn lifecycle and pending steering injection | `run_loop` | `emits_events_with_agent_message_types`, differential `steering` |
| `model_request_start/end` around streaming | `run_loop` | exact event sequence in `stops_after_current_turn_when_should_stop_after_turn_returns_true` |
| early exit on `error`/`aborted`/`length` | same | differential `error_stop`, `length_stop` |
| tool batch execution + result persistence | `execute_tool_calls*` | `handles_tool_calls_and_results`, differential `tools` |
| `end_turn` continuation instruction when no tool calls | same | `continues_after_assistant_text_until_terminating_tool_result`, differential `end_turn` |
| `turn_end`, abort check, `prepareNextTurn`, `shouldStopAfterTurn`, steering poll | same | `stops_after_current_turn_when_should_stop_after_turn_returns_true` |
| follow-up queue drains only when the agent would stop | same | differential `follow_up`, `should_stop` |
| `throwIfAborted` / `awaitWithAbort` for stream acquisition, iteration, result, tools | `throw_if_aborted` / `await_with_abort` | `terminalizes_when_stream_acquisition_ignores_abort`, `terminalizes_when_stream_iteration_ignores_abort`, `agent_abort.rs` |
| `iterateWithAbort` iterator close on abort | dropped stream (no explicit close); TS closes the iterator | gap: Rust `EventStream` has no `return()`; dropping stops consumption. Producer-side cancellation is the caller's responsibility. |
| `prepareAssistantToolExecutions` persists `prepareExecution` metadata | same | `persists_prepared_agent_id_and_passes_it_to_execution`, differential `prepare_execution` |
| `streamAssistantResponse` transform → convert → request build | same | `handles_custom_message_types_via_convert_to_llm`, `applies_transform_context_before_convert_to_llm`, differential `convert_custom` |
| API key resolution (`getApiKey` then `config.apiKey`) | same | code inspection; no dedicated test |
| streaming events: start/updates/done/error, partial replacement | same | differential `text` (done path); `message_update` exercised by Agent abort tests |
| tool mode selection: sequential config, per-tool sequential, else parallel | same | `forces_sequential_*`, `allows_parallel_*`, differential `sequential_*` |
| parallel: preflight starts in source order, ends in completion order, results in source order | `JoinSet` with indexed results | `emits_tool_execution_end_in_completion_order_and_results_in_source_order`, differential `parallel_order` |
| sequential: per-call prepare/execute/finalize, break on abort | same | `forces_sequential_when_tool_declares_sequential_mode` |
| early termination: non-empty batch and every finalized `terminate === true` | `should_terminate_tool_batch` | `stops_after_tool_batch_when_every_result_terminates`, `continues_after_parallel_*`, differential `terminate_*` |
| `prepareToolCallArguments` shim before validation | same; non-object results are a typed error | `prepares_tool_arguments_for_validation` |
| tool-not-found / validation-error / before-hook-error immediate results | same | code inspection; validation differential in `pi-ai` |
| `beforeToolCall` block/reason/terminate | same | `stops_after_blocked_tool_call_when_before_hook_terminates`, `continues_after_mixed_batch_*`, differential `blocked_terminate` |
| `beforeToolCall` argument mutation without revalidation | `BeforeToolCallContext.args: Arc<Mutex<Value>>`; the loop reads the cell after the hook | `executes_mutated_before_tool_call_args_without_revalidation`, differential `before_mutation` |
| abort during before hook / before execution | same (`Operation aborted`) | code inspection; `agent_abort.rs` covers abort during execute |
| `executePreparedToolCall`: update callback, ignore-after-settle, abort race, thrown error → error result | same (updates spawned then awaited; `AtomicBool` acceptance flag) | `ignores_tool_updates_after_execution_settles` (Agent), `agent_abort.rs`, differential `tool_error`/`parallel_update` |
| `afterToolCall` field-by-field overrides, error → error result | same | `after_tool_call_can_mark_batch_as_terminating`, differential `after_tool_call` |
| result capping: text 10 KiB / 2000 lines, details preview | `cap_tool_result*` (UTF-16 slicing to mirror JS `slice`) | gap: no dedicated unit test; exercised only for small payloads |
| tool result message shape, `message_start`/`message_end` | same | `handles_tool_calls_and_results`, differential `tools` |

## Discrepancies / exclusions

- `AgentEventSink` is `BoxFuture<'static, ()>` per the binding decision, so listener errors raised inside `Agent`'s sink cannot abort the loop; TypeScript's `emit` would reject. Recorded as an accepted typed deviation.
- `prepareArguments` must return a JSON object; TypeScript accepts any runtime value and lets validation decide. Explicit typed exclusion.
- Sparse/`null` holes in assistant content cannot be represented; the loop treats content blocks as typed values.
- Tool-result text capping mirrors JS UTF-16 `slice` via `Vec<u16>`; lone surrogates (not representable in Rust `String`) become U+FFFD.
