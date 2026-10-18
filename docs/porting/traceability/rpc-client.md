# RPC client traceability

Pinned source: `packages/coding-agent/src/modes/rpc/rpc-client.ts` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

Proof: `tests/rpc_client.rs` (14 cases) drives real `node` child scripts over stdio (no model/network); the
in-module unit test injects the kill call to reach the forced-termination failure path, which the OS
cannot produce (`SIGKILL` always terminates). The TypeScript test files `rpc-client-process-exit.test.ts`,
`rpc-client-idle-events.test.ts`, and `rpc-client-clone.test.ts` are covered by these cases.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `constructor(options = {})` | Defaults: `cliPath` `dist/cli.js`, `--mode rpc`, optional `--provider`/`--model`, `nodeArgs` before the entry point, extra `args` last | `RpcClientOptions`, `spawn_child` |
| `start()` | Rejects `Client already started or stopping`; spawns `node`; collects stderr; waits 100 ms; rejects `RPC client stopped during startup. Stderr: …` or the exit error | `start`; `rejects_a_second_start`, `reports_startup_cancellation_when_stopped_during_startup`, `rejects_in_flight_request_when_child_exits` |
| `start()` spawn failure | `Agent process error: spawn node ENOENT. Stderr: …` | `spawn_child` + `spawn_error_message` |
| `stop()` / `stopProcess()` | Detaches the reader, rejects pending with `RPC client stopped. Stderr: …`, SIGTERM, SIGKILL after 1 s, rejects after another 1 s | `stop`, `stop_process`, `terminate`; `rejects_pending_request_when_stopped`, `sends_sigkill_after_sigterm_is_ignored_and_waits_for_exit`, unit test `stop_failure_retains_the_process_handle_and_reports_previous_failure` |
| `stop()` retry state | Failed stop keeps the process, `start()` then reports `Client already started or stopping`, `send()` reports `Previous RPC stop failed: …`; a later stop retries and clears the state | unit test `stop_failure_retains_the_process_handle_and_reports_previous_failure` |
| `onEvent(listener)` | Listener receives every non-correlated line in wire order; returns an unsubscribe function | `on_event` + `RpcEventSubscription` (unsubscribe on `unsubscribe()`/`Drop`); `delivers_unmatched_responses_as_events`, `sends_jsonl_commands_with_id_last_and_correlates_responses` |
| `getStderr()` | Collected child stderr | `stderr()`; `collects_child_stderr` |
| `send(command)` | Rejects while stopping/stopped/not started/exited; assigns `req_<n>`; appends `id` last; 30 s response timeout | `send`; `sends_jsonl_commands_with_id_last_and_correlates_responses` (exact line), `rejects_commands_sent_after_stop_begins` |
| `handleLine(line)` | Ignores non-JSON; resolves a pending request when `type === "response"` and `id` is truthy and pending; otherwise emits an event | `ClientInner::handle_line`, `response_id`; `delivers_unmatched_responses_as_events`, `treats_responses_with_an_empty_id_as_events` |
| `waitForIdle(timeout = 60000)` | Resolves on the first `agent_end` without `willRetry`; timeout `Timeout waiting for agent to become idle. Stderr: …` | `wait_for_idle`; `waits_through_retrying_agent_ends` |
| `collectEvents(timeout = 60000)` | Collects through the final `agent_end`; timeout `Timeout collecting events. Stderr: …` | `collect_events`/`EventCollector`; `collects_retry_events_through_the_final_agent_end` |
| `promptAndWait(message, images, timeout)` | Subscribes before prompting, then returns the collected events | `prompt_and_wait`; `collects_retry_events_through_the_final_agent_end` |
| `prompt`/`steer`/`followUp`/`abort`/`interrupt`/`setThinkingLevel`/`setSteeringMode`/`setFollowUpMode`/`setAutoCompaction`/`setAutoRetry`/`abortRetry`/`abortBash`/`setSessionName` | Send only; a `success: false` response is ignored | same-named methods; `rejects_commands_sent_after_stop_begins` |
| `newSession`, `getState`, `setModel`, `cycleModel`, `getAvailableModels`, `cycleThinkingLevel`, `getAvailableThinkingLevels`, `exportHtml`, `switchSession`, `fork`, `clone`, `getForkMessages`, `getEntries`, `getTree`, `getLastAssistantText`, `getMessages`, `getCommands` | `getData`: throw the response `error` on failure, otherwise return `data` | `response_data` + typed result structs; `surfaces_command_failure_messages`, `sends_jsonl_commands_with_id_last_and_correlates_responses` |
| `clone()` | Sends `{ type: "clone" }` and returns `data.cancelled` | `clone_session`; `clone_sends_the_clone_command_and_returns_cancelled` |
| `compact(customInstructions?)` | Sends `{ type: "compact", customInstructions }` (omitted when absent) and returns `data` | `compact` + `core::compaction::CompactionResult`; `compact_sends_custom_instructions_and_returns_the_result`, `compact_omits_absent_custom_instructions` |
| `getSessionStats()` | Sends `{ type: "get_session_stats" }` and returns `data` | `get_session_stats` + `core::agent_session::SessionStats`; `get_session_stats_returns_the_stats_payload` |
| `bash(command)` | Sends `{ type: "bash", command }` and returns `data` | `bash` + `core::bash_executor::BashResult`; `bash_sends_the_command_and_returns_the_result` |

## Exclusions and deviations

- Typed Rust APIs exclude invalid JavaScript runtime input types (decision 2026-08-12): response `data`
  is deserialized into the declared Rust type instead of being cast unchecked. A malformed response line
  rejects the pending request with `Malformed response: …` instead of resolving with an unchecked object;
  missing/invalid `data` rejects with `Missing response data for <command>` / `Invalid response data for
  <command>: …`. TypeScript would return `undefined` or a partial object.
- `getAvailableModels()` returns `Vec<Model>`: TypeScript declares `ModelInfo[]` but returns the full
  `Model` objects from `data.models` unchanged. Likewise `setModel()` returns the full `Model` although
  its annotation is `{ provider, id }`. Rust models the actual wire data.
- `onEvent()` returns an `RpcEventSubscription` guard (removal on `Drop`) instead of a closure; both
  remove the listener.
- `send()`/`stop()` futures start on first poll. Concurrent calls are serialized with `stop_gate` rather
  than sharing one in-flight stop promise; `tokio::join!` reproduces the TypeScript ordering used by the
  process-exit tests.
- A child stdin write failure is reported as `Agent process stdin error: <io error>. Stderr: …`; Node's
  `stdin.write` throws or emits `error` with its own message text.
- `getLastAssistantText()` returns `Option<String>`; TypeScript's `string | null` and a missing `text`
  both map to `None`.
- `wait_for_idle`/`collect_events` take an explicit `Duration`; `DEFAULT_EVENT_TIMEOUT` is the TypeScript
  60 s default.
- Test seam: `RpcClient::with_kill` injects the signal sender so the stop-failure path can be tested.
- `core::agent_session::SessionStats`, `SessionTokenStats`, and `ContextUsage` gained `Deserialize` so the
  typed client can parse `get_session_stats` data (additive derive only).
- No source bug found.
