# rpc-mode traceability

Q15 common-pin correction2026-10-23: session subscription serializes the complete original event, not print-mode JSON shaping. Old-only agent_settled is removed. Full actual-frozen RPC differential GREEN6/6 after literal prompt, context persistence and fork/clone error-boundary corrections; no record filtering.

Common-pin source: `packages/coding-agent/src/modes/rpc/rpc-mode.ts`, [source-lock](../source-lock.json).

Proof: `tests/differential_rpc_mode.rs` drives the real pinned TypeScript `runRpcMode` and the Rust
port through `tests/bin/pi_mode_harness.rs` over stdin/stdout with the faux provider, comparing every
response/event line byte-for-byte after substituting only ids, timestamps, durations, and temp paths. For clone after the unsaved context-only fork, each complete serialized response is asserted against the same literal `Entry ` + that runtime's already captured leaf ID + ` not found` template; generated error subjects are not normalized.
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
| `registerSignalHandlers` :390–407 | Install synchronously; `SIGTERM` → shutdown(143, SIGTERM), `SIGHUP` → shutdown(129, SIGHUP) | `register_signal_handlers`; `rpc_signal_sigterm_at_first_startup_turn_disposes_and_exits_143` and `rpc_signal_sighup_at_first_startup_turn_disposes_and_exits_129` actual-source/native process RED: native died by OS signal without disposal; source disposed and exited normally. `rpc-mode-startup-signal-red.log`; GREEN2/2 at38ca42f9 in `rpc-mode-startup-signal-green.log` |
| `prompt` | Emit the success response from the preflight callback, otherwise the error; never block the input loop | `start_prompt` on the local set; differential prompt response + events |
| `steer`, `follow_up`, `abort`, `interrupt`, `abort_retry`, `abort_bash` | Send-only or queued interrupt | `handle_command`; differential |
| `new_session`, `switch_session`, `fork`, `clone` | Runtime transition, rebind when not cancelled, exact data shapes | `handle_command`; differential (fork/clone/new_session) |
| `get_state` | Session snapshot in `RpcSessionState` field order | `session_state`; differential |
| `set_model`, `cycle_model`, `get_available_models` | Model runtime snapshot and `Model not found: <provider>/<id>` | `handle_command`; differential |
| `set_thinking_level`, `cycle_thinking_level` | `null` data when cycling is unsupported | `handle_command`; differential |
| `set_steering_mode`, `set_follow_up_mode` | Persisted queue modes | `handle_command`; differential |
| `compact`, `set_auto_compaction`, `set_auto_retry` | Compaction result / settings writes; manual-compaction failures surface as an error response | `handle_command`; differential scenario `rpc_mode_reports_manual_compaction_failures` |
| `bash` :584–589 | Call `executeBash` directly; no `user_bash` hook | `handle_command`; `rpc_audit_bash_does_not_emit_the_old_only_user_bash_hook` actual-source/native process RED before correction; GREEN pending |
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
- Input lines are handled on a `LocalSet` (one spawned task per line) because `export_to_html`
  holds a non-`Send` `Rc` renderer. Ordering of responses/events matches the differential.
- `get_available_thinking_levels` returning an unknown-command error is a preserved source bug.
- No other source bug found.


## Queue11 lifecycle audit (2026-10-24)

Actual-source/private-process REDs at8622023c plus fixtures: native startup exposed `isStdoutTakenOver=false` instead of true; native `bash` emitted a hook absent in frozen; rebinding removed a replacement session's unrelated observer, whose marker exists in frozen and is absent natively. Corrections use the existing takeover authority, owner-bound subscription IDs, and direct `execute_bash`; the old-only custom-operations RPC test is removed, replaced by source-grounded non-emission proof. `rpc-mode-audit-behavior-red.log`; GREEN3/3 at557de237 in `rpc-mode-audit-three-green.log`. The three first audit cases are GREEN3/3 at557de237 (`rpc-mode-audit-three-green.log`). First-startup-turn signal cases then reproduced a separate native registration race for both TERM/HUP; streams now acquire synchronously, with registration errors propagated instead of silently ignored. Remaining shutdown and other module branches remain OPEN.

## Additional queue11 error and shutdown boundaries

- Actual-source raw unknown string `odd\\ud800` is echoed in `command` and `error`; native previously silently collapsed it to undefined. `rpc-mode-unknown-raw-behavior-red.log`. Parent cleared `RpcResponse.command: Option<JsString>` and mechanical constructors/callers; command classification retains the raw label. GREEN pending.
- Repeated TERM during pending real extension disposal: source exits143, native previously hung because the signal receiver awaited the first shutdown. `rpc-mode-repeat-signal-behavior-red.log`. Receivers now remain responsive while shutdown awaits and skip repeated tracked-child cleanup after shutdown starts. Other-kind HUP remains a regression case with the actual frozen default-handler exit129; the draft assumption of OS-status -1 was falsified, not a native bug. GREEN pending.

## Queue11 callable-status prerequisite (2026-10-24)

Whole RPC audit remains open; baseline7/7 actual-source process tests GREEN at2235ec7d (rpc-mode-baseline.log). Source :174–181 evaluates a callable status with undefined before emitting setStatus; a throw propagates synchronously to its extension handler. Native now uses the canonical ExtensionStatus/CallbackError carrier. New real-session-start process fixture covers text/undefined/original TypeError with raw UTF16/code; compile-RED recorded before the missing public API was implemented. [Contract](../../specs/extension-status.md). Other command/signal/rebind audit candidates are not claimed fixed by this prerequisite.
