# Traceability: `packages/agent-core/src/agent-loop.ts`

Reference: immutable `4f9e6f8a4`. Rust: `crates/pi-agent-core/src/agent/agent_loop.rs`.
Current evidence and exclusions: [slice-1 proof ledger](agent-core-api-repin.md). Complete with approved owned-value/event-snapshot and typed-input exclusions; latest carrier regression batch retains43 loop tests and direct frozen differentials.

## Registered stream-acquisition propagation (2026-10-24)

New actual-source `model_registry_agent_stream_throw::low_level_default_loops_preserve_registered_rejection_without_text_conversion` exercises both agentLoop and agentLoopContinue with default dispatch, TypeError+properties, raw lone UTF16, and raw object values. Full events plus iteration/result rejections are compared; only independently bounded timestamps are masked. RED q15e-low-level-stream-rejection-red-2: native `.fail(StreamFailure::new(error.message()))` invents an Error/text projection where source :54/:85 fails with the original rejection. Native Agent.prompt's outer catch is separately GREEN atbcf84da0; it must not be copied into these no-catch exports. Low-level failures now retain the CallbackError authority through the existing StreamFailure wrapper, without message conversion. StreamFailure::new remains the legacy Error(message) constructor; its source exposes the exact carrier to typed Rust wrappers. All6actual-source/native loop/continue cases GREEN946208f0, renewed after fixture extraction at561c6693;43existing loop tests also GREEN.

| Frozen function/export | Rust | Behavioral proof |
|---|---|---|
| AgentEventSink | AgentEventSink | differential listener_*_loop rejection scenarios |
| agentLoop / runAgentLoop | agent_loop / run_agent_loop | agent_loop.rs lifecycle, text/tools/error/length differential scenarios |
| agentLoopContinue / runAgentLoopContinue | agent_loop_continue / run_agent_loop_continue | empty context, restored/partly completed batch, continue queue tests |
| restoredPendingToolMessage (public) | restored_pending_tool_message (root and module exports) | restoration tests, failed assistant replay regression |
| createAgentStream | create_agent_stream | streamed event/result lifecycle tests; Q18 `lossless_provider_errors::native_stream_failure_reaches_agent_result_without_display_projection` proves native error result; `tool_hook_failures_keep_native_units_in_result_messages` proves before/after hook human messages |
| runLoop | run_loop, start_turn, inject_pending_messages, request_assistant_message, run_turn_tools, finish_turn | steering/follow-up/end_turn/prepare_next_turn/should_stop differential; replaced context regression; `agent_verifier_regressions::steering` covers steering after an end-turn reminder |
| agentAbortError / throwIfAborted / awaitWithAbort / iterateWithAbort | run_aborted / throw_if_aborted / await_with_abort / consume_assistant_stream | acquisition/iteration abort and stream-failure tests; iterator-return gap below |
| prepareAssistantToolExecutions | prepare_assistant_tool_executions | prepare_execution differential, persisted identity tests, last duplicate definition regression |
| streamAssistantResponse | stream_assistant_response, consume_assistant_stream, finish_assistant_message | transform/convert/API-key/request lifecycle, partial/error/abort tests |
| executeToolCalls / Sequential / Parallel | execute_tool_calls / execute_tool_calls_sequential / execute_tool_calls_parallel | modes, ordered preflight/results, completion-order events, termination differentials |
| shouldTerminateToolBatch | should_terminate_tool_batch | terminate_all/terminate_mixed; blocking no longer terminates |
| prepareToolCallArguments / prepareToolCall | prepare_tool_call_arguments / prepare_tool_call / apply_before_tool_call | validation, before mutation/error/block; preserve raw call after preparation; empty blocked reason |
| executePreparedToolCall | execute_prepared_tool_call / run_prepared_tool / tool_update_callback / await_update_tasks | update settlement, abort ignoring tool, tool errors; rejection lifetime regression; `agent_verifier_regressions::lifetime` covers pending+rejected sinks and post-abort tool side effects |
| finalizeExecutedToolCall | finalize_executed_tool_call / apply_after_tool_call / apply_after_tool_call_result | after override/termination/errors; nullish details and result error-hint regressions |
| capFinalizedToolCallOutcome / capToolResult / Content / Details / stringify / Text / isTooLarge | cap_finalized_tool_call_outcome / cap_tool_result / cap_tool_result_text and helpers | existing byte/line/UTF-16/details capping tests + tool_result_cap differential; shared representation lane excluded |
| emitToolExecutionEnd / Start / createToolResultMessage / emitToolResultMessage | corresponding snake_case functions | exact differential event/message output |

Typed boundaries retain owned native UTF-16 messages and arbitrary argument/detail JSON. JSON-object tool argument shape remains an explicit typed-input exclusion. Native iterator-return callbacks now preserve synchronous/asynchronous close failures and continued producer-future lifetime, proven by nine frozen scenarios (agent-core-api-repin.md). Immutable partial snapshots/owned return values follow approved rules; no dropped-receiver-only cleanup claim remains.
