# coding-agent supervisor traceability

Pinned source: `packages/coding-agent/src/supervisor/*.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.
Rust: `crates/pi-coding-agent/src/supervisor/` (`approval_reviewer.rs`, `response_tool.rs`, `service.rs`,
`client.rs`, `main.rs`). Focused tests: `crates/pi-coding-agent/tests/supervisor.rs` (13), `tests/differential_supervisor_request_loop.rs` (1).

## Q18 persisted message views

Supervisor main retains each `SessionEntry::as_message()` borrowed/owned view while reading its message. This supports raw unknown-entry projection without borrowing a temporary; native response content remains unchanged. Existing supervisor request-loop tests await coding-library compilation; this lifetime correction is not runtime proof.

## approval-reviewer.ts

| TypeScript source | Contract | Rust and proof |
|---|---|---|
| `reviewToolCallWithSupervisor` | approve → undefined; reject → block unless escalation; error/invalid → human or block; a rejected decision request or human reviewer propagates | `review_tool_call_with_supervisor` (`SupervisorHumanReviewer` is fallible); `allows_approved_calls_and_blocks_rejected_calls`, `escalates_supervisor_errors_to_the_human_reviewer`, `escalates_rejection_only_for_the_ask_preset`, `reviewer_failures_propagate_instead_of_allowing_the_tool_call` |

## response-tool.ts

| TypeScript source | Contract | Rust and proof |
|---|---|---|
| `SUPERVISOR_RESPONSE_TOOL_NAME`, prompt metadata | exact literals | same-named constants |
| `supervisorResponseParameters` | TypeBox object schema in source property order | `supervisor_response_parameters`; asserted indirectly by the parse/prompt tests |
| `execute` | advisory trimmed answer, else acknowledgement | `supervisor_response_text` |
| `createSupervisorResponseTool` | tool registration | `create_supervisor_response_tool`; metadata/execution differential in `tests/differential_supervisor_response.rs` |

## service.ts

Current common-pin whole-module proof: [supervisor-service](supervisor-service.md). The following group table is historical.

| TypeScript source | Contract | Rust and proof |
|---|---|---|
| `buildSupervisorPrompt` | exact instruction block + 2-space JSON request | `build_supervisor_prompt`; `builds_bounded_prompts_for_each_request_kind` + differential `differential_supervisor_service` (5 request kinds) |
| `parseSupervisorResponse` | per-kind validation, string/content text inputs, branch key order | `parse_supervisor_response` + custom `Serialize`; `parses_responses_per_request_kind` + differential (15 parse cases, JSON strings) |
| `runSupervisorRequest` | durable completion, deadline expiry, approval preemption, claim-token errors | `run_supervisor_request`; `completes_a_request_with_the_parsed_response`, `expired_requests_complete_with_the_deadline_error` |

## client.ts

| TypeScript source | Contract | Rust and proof |
|---|---|---|
| `requestSupervisorDecision` | post + wake, poll to deadline, retry with jitter, attempt suffix | `request_supervisor_decision`; `client_waits_for_the_durable_response`, `client_reports_the_attempt_suffix_when_every_attempt_expires` |

## main.ts

| TypeScript source | Contract | Rust and proof |
|---|---|---|
| `SUPERVISOR_SESSION_ID`, `SUPERVISOR_COMPACTION_PERCENT`, `SUPERVISOR_TOOL_NAMES`, `SUPERVISOR_EXCLUDED_TOOL_NAMES` | exact lists | same-named constants |
| `SupervisorConsolePromptQueue` | FIFO enqueue/take | `SupervisorConsolePromptQueue` |
| `createSupervisorSettingsManager` | in-memory auto-approve + full-access | `create_supervisor_settings_manager` |
| `validateSupervisorExtensionLoad` | `path: error` join | `validate_supervisor_extension_load`; `extension_load_diagnostics_are_rejected_before_startup` |
| `blockSupervisorFileAccess` | KB-only read/edit/write, symlink-aware | `block_supervisor_file_access`; `blocks_file_access_outside_the_kb_and_symlink_escapes` |
| `createSupervisorConsoleSnapshot` | service/session/cwd/generation/branch | `create_supervisor_console_snapshot` |
| `openSupervisorSession` | newest `_supervisor.jsonl`, metadata attachment, archive | `open_supervisor_session` |
| `runSupervisorRequestLoop` | recover → claim → console prompt → wake | `run_supervisor_request_loop` |
| `processSupervisorRequest` | evaluation with abort, compaction at 75%, boundary-scoped response | `process_supervisor_request`, `evaluate_supervisor_prompt`; `process_request_does_not_reuse_a_prior_assistant_response` |
| `readCurrentSupervisorResponse` | boundary, structured tool call, end_turn fallback, stop reason, text join | `read_current_supervisor_response`; `structured_tool_calls_are_read_without_assistant_text`, `end_turn_tool_use_falls_back_to_the_last_stopped_assistant_message`, `assistant_text_with_a_structured_response_is_rejected` |
| `createSupervisorResourceLoader` | gate + remote-compaction inline extensions, no discovery, load validation | `create_supervisor_resource_loader`; `resource_loader_registers_the_kb_gate_and_remote_compaction` |
| `createSupervisorSession` | sol model, tool allowlist, custom response tool, exclusions, low thinking | `create_supervisor_session`; `supervisor_session_uses_the_sol_model_and_supervisor_tool_set` |
| `runSupervisorService` | session open, model runtime, wake + console lifecycle, request loop, abort on SIGINT/SIGTERM | `run_supervisor_service` |
| `runSupervisorRequestLoop` | recover → claim → process → console prompt → wake | `run_supervisor_request_loop`; differential `differential_supervisor_request_loop` (control rows + session file) |

## Q18 main.ts native checkpoint

Common pin: `4f9e6f8a4`. Console queue IDs/text, snapshots, startup paths/errors, extension diagnostics, request-loop claim tokens/DB paths and session/leaf IDs retain native UTF-16. File APIs receive encoded paths only at OS calls. Main-owned service/client DB-path fields are tracked in the lane prerequisite inventory; no scalar adapter is added.

`tests/lossless_supervisor_main.rs` asserts raw extension diagnostics and queue/snapshot values. Execution and legacy supervisor-test adaptation remain pending coding-library compilation; this checkpoint is not runtime parity evidence.

## Gaps

- `runSupervisorService` is process-level (signal handlers, real model runtime); its wiring is compiled and exercised
  indirectly through the resource-loader and session tests. The request loop is differentially covered.
- Service evaluator cleanup now follows the source AbortSignal/await contract; current proof is in [supervisor-service](supervisor-service.md).

## Discrepancies

None.
