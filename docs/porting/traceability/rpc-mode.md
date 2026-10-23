# rpc-mode traceability

Pinned source: `packages/coding-agent/src/modes/rpc/rpc-mode.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Proof: `tests/differential_rpc_mode.rs` drives the real pinned TypeScript `runRpcMode` and the Rust
port through `tests/bin/pi_mode_harness.rs` over stdin/stdout with the faux provider, comparing every
response/event line byte-for-byte after substituting only ids, timestamps, durations, and temp paths.
The sequence covers every command the mode implements plus unknown and malformed commands, `bash`,
`export_html`, `fork`, `clone`, `new_session`, and `get_entries` with a `since` cursor. Two further
scenarios cover the extension UI dialog round-trip (tool approval select answered with `Allow once`)
and the restored-session continuation. Six unit tests in `modes/rpc/rpc_mode.rs` cover the extension UI
dialog broker.

| TypeScript source | Observable behavior | Rust mapping and proof |
|---|---|---|
| `takeOverStdout` + `output` | Every response, event, and UI request is one compact JSON line | `RpcState::output` + `serialize_json_line`; every differential line |
| `success` / `error` | `{id, type, command, success, data?, error?}` with `data` omitted when `undefined` | `RpcResponse::success*` / `error`; every differential response |
| `pendingExtensionRequests` + `createDialogPromise` | Register id, emit request, resolve on response/timeout/abort, default on timeout/abort | `DialogBroker` + `create_dialog_promise`/`await_dialog`; unit tests `dialog_resolves_the_parsed_client_response`, `dialog_times_out_to_the_default_value`, `pre_cancelled_dialogs_resolve_without_emitting_a_request`, `cancelled_dialogs_resolve_with_the_default_value`, `unknown_response_ids_are_ignored` |
| `createExtensionUIContext` dialogs (`select`, `confirm`, `input`, `editor`) | Request payloads and response parsing | `RpcUiContext` implementations; dialog unit tests assert the select payload shape; the approval differential answers a real `select` request |
| `notify`, `setStatus`, `setWidget`, `setTitle`, `setEditorText`, `pasteToEditor` | Fire-and-forget requests with a fresh uuid | `RpcUiContext`; request shapes come from `rpc_types` (differentially covered by `rpc_types.rs`) |
| `requestRender`, working/editor/footer/theme/autocomplete/custom no-ops | No output | trait defaults; `set_theme` returns `Theme switching not supported in RPC mode` (unit test) |
| `setRebindSession` + `rebindSession` | Bind UI context, mode `rpc`, control DB path, command-context actions, shutdown handler, `onError`; resubscribe | `set_rebind_session`, `rebind_session`; differential exercises rebind through `fork`/`clone`/`new_session` |
| `registerSignalHandlers` | `SIGTERM` → shutdown(143, SIGTERM), `SIGHUP` → shutdown(129, SIGHUP) | `register_signal_handlers`; process-level, not covered by an automated test |
| `prompt` | Emit the success response from the preflight callback, otherwise the error; never block the input loop | `start_prompt` on the local set; differential prompt response + events |
| `steer`, `follow_up`, `abort`, `interrupt`, `abort_retry`, `abort_bash` | Send-only or queued interrupt | `handle_command`; differential |
| `new_session`, `switch_session`, `fork`, `clone` | Runtime transition, rebind when not cancelled, exact data shapes | `handle_command`; differential (fork/clone/new_session) |
| `get_state` | Session snapshot in `RpcSessionState` field order | `session_state`; differential |
| `set_model`, `cycle_model`, `get_available_models` | Model runtime snapshot and `Model not found: <provider>/<id>` | `handle_command`; differential |
| `set_thinking_level`, `cycle_thinking_level` | `null` data when cycling is unsupported | `handle_command`; differential |
| `set_steering_mode`, `set_follow_up_mode` | Persisted queue modes | `handle_command`; differential |
| `compact`, `set_auto_compaction`, `set_auto_retry` | Compaction result / settings writes; manual-compaction failures surface as an error response | `handle_command`; differential scenario `rpc_mode_reports_manual_compaction_failures` |
| `bash` | `user_bash` extension event first, then `executeBash` | `run_bash`; differential (`echo hello`) |
| `get_session_stats`, `export_html`, `get_entries`, `get_tree`, `get_last_assistant_text`, `get_messages`, `get_commands`, `get_fork_messages`, `set_session_name` | Exact data shapes; `leafId: null` for empty sessions; `text` omitted when absent; `Entry not found: <id>`; `Session name cannot be empty` | `handle_command`; differential |
| `handleInputLine` parse failure | `Failed to parse command: <V8 message>` | `js_json::parse`; differential malformed line |
| `handleInputLine` unknown command | `Unknown command: <type>` (also `undefined` when `type` is not a string) | `deserialize_command` + `RpcResponse::error_optional`; differential |
| `extension_ui_response` routing | Resolve the pending request and emit nothing | `route_extension_ui_response`; unit test |
| `shutdown` | Cleanup, dispose, detach input, flush unless SIGTERM, `process.exit` | `shutdown` |
| Input end | `shutdown(0)` | channel close → `shutdown`; differential exit code |
| `consumeResumeContinuationRequest` / `shouldContinueInterruptedSession` | Continue a restored session, logging failures to stderr | `resume_interrupted_session`; differential scenario `rpc_mode_continues_an_interrupted_restored_session`, plus 6 predicate tests in `tests/session_continuation.rs` |

## Fixed while porting

- `CompactionEndEvent.result` serialized as `null` when absent; the source omits `undefined`.
  `result` now skips serialization when `None` (pinned by the compaction differential scenario).
- `should_continue_interrupted_session(&[])` panicked (`..=usize::MAX` slice) in the agent-session
  port; the source returns `false`. Fixed to `..(last_index + 1).max(0)` and pinned by
  `empty_message_list_does_not_continue`; RPC startup on a fresh session exercises it.

## Source discrepancies

- `rpc-types.ts` and `rpc-client.ts` declare and send `get_available_thinking_levels`, but
  `rpc-mode.ts` has no case for it, so the server answers
  `Unknown command: get_available_thinking_levels`. The Rust mode reproduces the response and the
  differential pins it.

## Exclusions and deviations

- Typed command parsing replaces the TypeScript switch: a command whose JSON shape does not match its
  variant rejects with the `serde_json` message instead of a JavaScript `TypeError` (decision
  2026-08-12, invalid runtime input types excluded).
- `UserBashEventResult.operations` is stored as JSON by the extensions port, so extension-provided
  custom bash operations cannot be passed to `executeBash`; the mode runs local operations. The
  event-provided `result` is deserialized into `BashResult` and recorded. Documented gap owned by the
  extensions slice.
- Input lines are handled on a `LocalSet` (one spawned task per line) because `export_to_html`
  holds a non-`Send` `Rc` renderer. Ordering of responses/events matches the differential.
- `get_available_thinking_levels` returning an unknown-command error is a preserved source bug.
- No other source bug found.
