# rpc-mode traceability

Current whole-module checkpoint: [proof](../rpc-mode-proof.md). Actual frozen/native process29/29 and owned unit/surface/process-oracle9/9 GREEN; approved gaps are enumerated per field below. Earlier prerequisite/baseline notes are historical, not completion evidence.

## Native stdin pause boundary (2026-10-24)

Frozen :731–735 detaches input listeners and calls `process.stdin.pause()`. Shared native JSONL detach now disables delivery but preserves/drains the owned transport, matching Node's flowing helper. Tokio stdin has no equivalent cancellation/pause of an already-started blocking read, so OS backpressure while the RPC mode is paused is an explicitly approved Node/libuv runtime-oddity gap (main2026-10-24). Observable records remain exact: detached input never reaches dispatch. No generic pause/backend or new test exclusion was added.

## SIGHUP detach/flush correction (2026-10-24)

After parent4c28cdf4, the requested process matrix passed28/29: the fire-fixture output was unchanged, but native startup SIGHUP occasionally exited0. Root: signal shutdown detached the line reader, which dropped the channel sender; the input loop misclassified that intentional detach as stdin EOF and called shutdown0 while the129 path awaited stdout flush. Frozen :731–738 removes `onInputEnd` before flushing. The native input loop now permits an EOF shutdown only while the input subscription remains attached, and otherwise retains the frozen pending lifetime.

`rpc_sighup_detach_does_not_synthesize_eof_while_stdout_flush_is_blocked` supplies a real512KiB notification and holds both private stdout pipes closed to readers. An extension command emits the notification then signals HUP after normal mode binding. Both reach disposal; frozen stays alive awaiting drain while pre-fix native exits prematurely. After release, the same test requires129 and the complete notification. The first startup-flood draft blocked frozen binding before signal registration and was rejected as an invalid fixture; the command-boundary RED is the accepted reproduction. Focused GREEN and final checkpoint gates remain pending. Logs: `/tmp/pi-rust-repin-lossless-interactive/rpc-mode-blocked-flush-*`.

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
| `pendingExtensionRequests` + `createDialogPromise` | Register id, emit request, resolve on response/timeout/abort, default on timeout/abort | `DialogBroker` + `create_dialog_promise`/notification-settled broker; unit tests `dialog_resolves_the_parsed_client_response`, `dialog_times_out_to_the_default_value`, `pre_cancelled_dialogs_resolve_without_emitting_a_request`, `cancelled_dialogs_resolve_with_the_default_value`, `unknown_response_ids_are_ignored` |
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
| `bash` :584–589 | Call `executeBash` directly; no `user_bash` hook | `input::dispatch_operation`; `rpc_audit_bash_does_not_emit_the_old_only_user_bash_hook` actual-source/native process RED before correction; GREEN at557de237 |
| `get_session_stats`, `export_html`, `get_entries`, `get_tree`, `get_last_assistant_text`, `get_messages`, `get_commands`, `get_fork_messages`, `set_session_name` | Exact data shapes; `leafId: null` for empty sessions; `text` omitted when absent; `Entry not found: <id>`; `Session name cannot be empty` | `handle_command`; differential |
| `handleInputLine` parse failure | `Failed to parse command: <V8 message>` | `js_json::parse`; differential malformed line |
| `handleInputLine` unknown command | Echo the raw JSON tag/id and JS String error subject; ToPrimitive throws enter the command catch | `input::handle_command` and raw wire response; `rpc_external_unknown_command_echoes_original_json_values`, raw UTF16 and raw id tests |
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

- Superseded old typed-command parsing exclusion: external server ingress now uses literal JsValue dispatch. The 2026-10-24 per-field conversion/rejection ruling and complete field matrix below govern native core boundaries; client DTOs remain typed.
- Input lines are handled on a `LocalSet` (one spawned task per line) because `export_to_html`
  holds a non-`Send` `Rc` renderer. Ordering of responses/events matches the differential.
- `get_available_thinking_levels` returning an unknown-command error is a preserved source bug.
- No other source bug found.


## Queue11 lifecycle audit (2026-10-24)

Actual-source/private-process REDs at8622023c plus fixtures: native startup exposed `isStdoutTakenOver=false` instead of true; native `bash` emitted a hook absent in frozen; rebinding removed a replacement session's unrelated observer, whose marker exists in frozen and is absent natively. Corrections use the existing takeover authority, owner-bound subscription IDs, and direct `execute_bash`; the old-only custom-operations RPC test is removed, replaced by source-grounded non-emission proof. `rpc-mode-audit-behavior-red.log`; GREEN3/3 at557de237 in `rpc-mode-audit-three-green.log`. The three first audit cases are GREEN3/3 at557de237 (`rpc-mode-audit-three-green.log`). First-startup-turn signal cases then reproduced a separate native registration race for both TERM/HUP; streams now acquire synchronously, with registration errors propagated instead of silently ignored. Remaining branches are covered by the later audit sections below.

## Additional queue11 error and shutdown boundaries

- Actual-source raw unknown string `odd\\ud800` is echoed in `command` and `error`; native previously silently collapsed it to undefined. `rpc-mode-unknown-raw-behavior-red.log`. Parent cleared `RpcResponse.command: Option<JsString>` and mechanical constructors/callers; command classification retains the raw label. GREEN in final process29/29 and owned module9/9; see current checkpoint proof.
- Repeated TERM during pending real extension disposal: source exits143, native previously hung because the signal receiver awaited the first shutdown. `rpc-mode-repeat-signal-behavior-red.log`. Receivers now remain responsive while shutdown awaits and skip repeated tracked-child cleanup after shutdown starts. Other-kind HUP remains a regression case with the actual frozen default-handler exit129; the draft assumption of OS-status -1 was falsified, not a native bug. GREEN in final process29/29 and owned module9/9; see current checkpoint proof.

## Queue11 callable-status prerequisite (2026-10-24)

Callable prerequisite history: baseline7/7 actual-source process tests GREEN at2235ec7d (rpc-mode-baseline.log). Source :174–181 evaluates a callable status with undefined before emitting setStatus; a throw propagates synchronously to its extension handler. Native now uses the canonical ExtensionStatus/CallbackError carrier. New real-session-start process fixture covers text/undefined/original TypeError with raw UTF16/code; compile-RED recorded before the missing public API was implemented. [Contract](../../specs/extension-status.md). Other command/signal/rebind audit candidates are not claimed fixed by this prerequisite.

## 2026-10-24 external-ingress ruling and field matrix

The parent approved replacing the server's typed command filter with literal `JsValue` dispatch; native client DTOs stay typed. (a) RPC-local property access, strict model/id lookup, truthiness, trim and ToString/ToPrimitive throws are exact. This includes raw `id`/`type` echoes, provider/modelId error subjects, `get_entries.since`, `set_session_name.name`, and falsey `new_session.parentSession`. External raw values are not excluded wholesale by the native-core typed-API rule.

(b) First core operations, using shared truthiness authority:

| Field | Frozen first use | Native boundary |
|---|---|---|
| `steer.images` | agent-session.ts:3416–3420, `if(images)` before spreading | Falsey values produce no images; truthy image arrays remain exact |
| `follow_up.images` | `_queueFollowUp`, `if(images)` before spreading | Same falsey conversion |
| `export_html.outputPath` | core/export-html/index.ts:275, truthy path or default | Falsey values select the default; truthy strings stay exact |

(c) Approved per-field gaps under the 2026-10-24 ingress ruling: missing required values and unrepresentable deep core values reject explicitly with `RPC field <field> requires <shape> at the typed core boundary`; they are never coerced or silently dropped. Optional missing properties remain absent. These are boundary gaps, not claims about frozen's failure messages/side effects for those invalid deep inputs.

| Field | Accepted native domain; explicit rejection gap | Frozen core boundary |
|---|---|---|
| `prompt.message` | UTF-16 string | agent-session.ts:3014/3233; command expansion, recovery and prompt pipeline |
| `prompt.images` | Image array, absent | :3064; extension input handlers and prompt content |
| `prompt.streamingBehavior` | `steer`/`followUp`, absent | prompt streaming/queue options |
| `steer.message` | UTF-16 string | :3360; startsWith/expansion/queue |
| `steer.images` | Reject truthy non-image arrays/values | :3416; spread into typed content |
| `follow_up.message` | UTF-16 string | :3393; startsWith/expansion/queue |
| `follow_up.images` | Reject truthy non-image arrays/values | `_queueFollowUp`; spread into typed content |
| `new_session.parentSession` | Truthy UTF-16 string; falsey handled locally | runtime.newSession/session-manager construction |
| `set_thinking_level.level` | Native ThinkingLevel enum | :4230; capability clamp, persistence, publication |
| `set_steering_mode.mode` | `all`/`one-at-a-time` | :4314; store in agent and settings |
| `set_follow_up_mode.mode` | `all`/`one-at-a-time` | :4323; store in agent and settings |
| `compact.customInstructions` | UTF-16 string, absent | :4337; extension hook, compaction, summary instructions |
| `set_auto_compaction.enabled` | Boolean | :5156; stored unchanged by settings |
| `set_auto_retry.enabled` | Boolean | :5791; stored unchanged by settings |
| `bash.command` | UTF-16 string | :5807; shell prefix/execution/history |
| `bash.excludeFromContext` | Boolean, absent | :5841; stored in bash message |
| `export_html.outputPath` | Reject truthy non-string | export-html/index.ts:275; normalizePath/filesystem |
| `switch_session.sessionPath` | UTF-16 string | runtime.switchSession/restore/path handling |
| `fork.entryId` | UTF-16 string | runtime.fork/session lookup/lifecycle |

Image representability ruling (parent2026-10-24): reject, do not project or drop. `data` is base64 and `mimeType` is opaque; their unpaired-unit rejection follows the scalar approval. A lossless unknown-property carrier is a separate pi-ai item.

| Field | Approved gap (2026-10-24 image representability ruling) |
|---|---|
| `prompt.images[].type` | Reject non-image/non-scalar tag |
| `prompt.images[].data` | Reject non-string or unpaired units explicitly |
| `prompt.images[].mimeType` | Reject non-string or unpaired units explicitly |
| `prompt.images[].<unknown property>` | Exact retention through the shared [ImageContent carrier](../../specs/image-content-properties.md); retired rejection gap |
| `steer.images[].type` | Reject non-image/non-scalar tag |
| `steer.images[].data` | Reject non-string or unpaired units explicitly |
| `steer.images[].mimeType` | Reject non-string or unpaired units explicitly |
| `steer.images[].<unknown property>` | Same shared decoder/carrier; retired unknown-property rejection gap |
| `follow_up.images[].type` | Reject non-image/non-scalar tag |
| `follow_up.images[].data` | Reject non-string or unpaired units explicitly |
| `follow_up.images[].mimeType` | Reject non-string or unpaired units explicitly |
| `follow_up.images[].<unknown property>` | Same shared decoder/carrier; retired unknown-property rejection gap |

Implementation: `rpc_mode/input.rs`; no typed-command fallback remains. Tests: `rpc_external_unknown_command_echoes_original_json_values`, `rpc_external_model_lookup_uses_strict_raw_value_equality`, `rpc_external_local_trim_and_since_use_frozen_js_operations`, `rpc_external_core_boundaries_reject_instead_of_coercing_or_dropping`, and falsey-images/export tests. Initial RPC-local RED2/2 is `rpc-mode-external-json-red.log`; GREEN in final process29/29 and owned module9/9; see current checkpoint proof.

## Raw extension UI response boundary

`rpc-mode-ui-precedence-red.log`: actual source cancellation wins over coexisting value/confirmed; typed native untagged response parsing chose the other property and returned wrong values. The private broker now carries the complete `JsValue`; routing uses strict generated-string id lookup. Missing selected fields resolve defaults immediately, not a silently ignored response. Native client DTOs are unchanged. GREEN in final process29/29 and owned module9/9; see current checkpoint proof.

`cancelled` uses shared JS truthiness before inspecting the return field (RPC-local exact behavior, not a Boolean conversion of the returned confirmed/value). The following (c) rows are approved by the 2026-10-24 ingress/return-boundary ruling: source returns the raw field unchanged, with no `!!`, equality or String conversion. Unrepresentable return values yield an explicit `Result` error naming the field; they are never silently defaulted.

| Field | Frozen line/operation | Accepted domain / explicit gap |
|---|---|---|
| `select.extension_ui_response.value` | rpc-mode.ts:141, `r.value` unchanged | String or absent; other selected values reject |
| `confirm.extension_ui_response.confirmed` | :146, `r.confirmed` unchanged | Boolean or absent; other selected values reject |
| `input.extension_ui_response.value` | :151, `r.value` unchanged | String or absent; other selected values reject |
| `editor.extension_ui_response.value` | :272, `resolve(response.value)` unchanged | String or absent; other selected values reject |

Proofs: real extension-command round-trip `rpc_ui_responses_apply_cancellation_before_value_and_preserve_raw_text`, `rpc_ui_empty_replies_resolve_missing_field_defaults`; pure return-value coverage `raw_dialog_return_boundaries_reject_unrepresentable_values_explicitly` includes cancellation winning even over unrepresentable values.

## Dialog timer normalization

Source :116–119 schedules a truthy timeout through Node setTimeout. Native now uses shared `node_timers::normalize_timeout_delay` after the frozen truthiness filter; the original numeric request field remains unchanged. Actual-source/private-process overflow2147483648 RED proves source resolves default at1ms while the old native timer never settles within2s (`rpc-mode-timer-overflow-red.log`). GREEN in final process29/29 and owned module9/9; see current checkpoint proof.

Gap (parent2026-10-24, existing Node/V8/libuv runtime-oddity ruling): native omits Node TimeoutOverflowWarning and does not reproduce OS timer precision. The process proof asserts the source warning and native empty stderr separately; no warning filtering, suppression or invented warning subsystem.

## Dialog first-notification settlement

Actual frozen-process oracle `oracle/rpc-dialog-order-oracle.mts` drives real runRpcMode and routes a response/abort synchronously before Promise continuations. The ordered native helper proof reproduced lost response-first values (32 notifications: both Some(answered) and None) in `rpc-mode-dialog-order-red.log`. A biased select is insufficient because abort-first must also win. The broker now removes/sends its one pending slot at response, abort-listener or timer notification; subsequent notifications are ignored. Awaiting only consumes that already chosen result; shared AbortSubscription/timer handles provide cleanup. No new dependency or error-carrier panic is added. GREEN in final process29/29 and owned module9/9; see current checkpoint proof.

## No-op surface and Theme object overload

The actual frozen RPC surface oracle executes ignored widget/header/footer/editor/autocomplete/custom/input factories (they must not run), working/render/tool-expansion no-ops and all synchronous getters. Both source setTheme(string) and setTheme(Theme) return `Theme switching not supported in RPC mode` (:306–310). Native previously inherited `UI not available` for the object overload; actual-source/native RED is `rpc-mode-ui-surface-behavior-red.log`. Both native entry points now use one rejection authority; no broader theme behavior change. GREEN in final process29/29 and owned module9/9; see current checkpoint proof. The first fixture attempt lacked normal entry-point theme initialization; it is not the behavioral RED.

## Shutdown-request discrepancy

| Classification | Frozen behavior | Native/proof |
|---|---|---|
| DISCREPANCY, preserve (parent2026-10-24) | `shutdownRequested` is polled only after input handling (:749–789); an async extension-command ctx.shutdown does not independently wake this check, so another command/EOF is needed | Literal flag/check retained; `rpc_extension_shutdown_request_is_checked_after_the_next_command` asserts the next get_state response and exit0 on both runtimes. Draft immediate-exit expectation failed on source itself (`rpc-mode-extension-shutdown-attribution.log`) and was never acceptance evidence |

`rpc_switch_session_restores_the_original_session_and_rebinds_events` adds a valid persisted-session switch round-trip, asserting restored identity/message count; prior whole-command matrix covers new/fork/clone/error/query/settings paths. Effect-bearing ingress handlers are named `dispatch_*`; manual readability audit uses explicit field boundaries and domain-sized matches, not an opaque DTO conversion pipeline. Rust-code-analysis binary is unavailable on this host; no metric pass is claimed.

## Parallel process-fixture publication

Final process integration first ran29 cases concurrently:19 passed and10 native children failed before RPC startup with missing theme/dark.json. `mode_harness::create_package_dir` reused one length-padded directory and used directory existence as a completeness signal while another test was copying files. The helper now creates a unique PID/atomic-sequence package directory per invocation, completes every theme copy before spawning children, and preserves the exact requested path length for token estimates. No protocol assertion, normalizer, production behavior, or test concurrency is relaxed. `rpc-mode-final-process.log` is the original fixture RED; post-fix integration29/29 GREEN ataf555494 (`rpc-mode-final-process-isolated.log`). Owned lib module9/9 passed at that integration revision.

## Historical intermittent observation timeout (acceptance 2026-10-26)

The one-off frozen RPC observation failure (empty source stderr, before import) is a deadline artifact: 19/20 exact-test runs sharing one CPU (`taskset -c 0`) fail identically at the 20 s deadline, while 40/40 ordinary runs pass. It falls under the accepted scheduling/timing gap category; no product change. Evidence: /tmp/accept-intermittents/REPORT.md.
