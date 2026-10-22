# agent-session.ts traceability

Rust: `crates/pi-coding-agent/src/core/agent_session.rs` (module `core::agent_session`).

## Q18 native error/path consumers

Provider human errors, auto-retry start/end error fields, compaction errors and required-auth failures stay native. AgentSession config/inner cwd, agent_dir, control_db_path, session-file accessors and export JSONL path/header values use JsString. Runtime persistence identity comparisons remain raw; export bytes encode only at the actual FS call. `lossless_agent_session_paths` contains a TS-created raw-session-path fixture through AgentSession and MultiAgentStore persistence identity; coding execution remains blocked by concurrent caller/helper migrations. No parent completion claim. Tree targets, leaf/cache IDs, labels, custom-entry and label callbacks remain native; Cow-backed entry projections are owned before crossing guards. Missing tree/materialization entry errors interpolate raw IDs. Materialization checks raw checkpoint provider/api/format/value fields, including the frozen array-value requirement, without decoding unrelated JSON. The same blocked target asserts raw missing-ID error units and reopen identity. Latest compile diagnostic `agent-session-path-compile-3.jsonl` reduced AgentSession primaries from 66 to 49; this is not runtime proof.

## Exported symbols

| TS | Rust | Tests |
| --- | --- | --- |
| `AgentSession` | `AgentSession` | `tests/agent_session.rs` (42), `tests/differential_agent_session.rs` |
| `AgentSessionConfig` | `AgentSessionConfig` | same |
| `AgentSessionEvent` | `AgentSessionEvent` + `AgentMessageEvent` | event assertions + differential |
| `AgentSessionEventListener` | `AgentSessionEventListener` | — |
| `PromptOptions` | `PromptOptions` | prompt/steer tests |
| `ModelCycleResult` | `ModelCycleResult` | cycle tests, differential |
| `SessionStats` / `ContextUsage` | same | stats/context tests, differential |
| `SupervisorDecisionRequester` | same | LLM reviewer path |
| `getAssistantMessageText` | `get_assistant_message_text` | duplicate/stat paths |
| `shouldContinueInterruptedSession` | `should_continue_interrupted_session` | unit helper |
| `isPermissionPromptProtocolTool` | `is_permission_prompt_protocol_tool` | discovery tests |
| `DETACHED_TOOL_CALL_COMPLETION_CUSTOM_TYPE`, `buildDetachedToolCallCompletionEntry` | `DETACHED_TOOL_CALL_COMPLETION_CUSTOM_TYPE`, `build_detached_tool_call_completion_entry` | `tests/agent_session_runtime_mailbox.rs` (detached completion delivery) |
| `bindAgentSessionMutationTargetResolver` | `bind_agent_session_mutation_target_resolver` | `tests/agent_session_mutation_target.rs` |
| `detachRunningTool` / `detachBashTool` / `hasDetachableTool` / `hasDetachableBashTool` | `detach_running_tool` / `detach_bash_tool` / `has_detachable_tool` / `has_detachable_bash_tool` | `tests/agent_session.rs::detach_running_tool_and_detach_bash_tool_delegate_to_the_tool_detach_registry` |
| `getToolDetachRegistry` context action | `get_tool_detach_registry` in `build_extension_context_actions` | same test reads the registry through `ExtensionContext::tool_detach_registry` |

## Behavior coverage

- Constructor: role/capability validation, tool registry, system prompt rebuild, hooks,
  extension runner construction, control-db path installation, permission rule store,
  supervisor requester default, detached-job lifecycle binding.
- Base tools: `createAllToolDefinitions` rebuilt on every `_buildRuntime` from current
  settings (read auto-resize, bash command prefix/shell path/detach registry/background
  jobs, live session environment); `baseToolsOverride` path appends `end_turn`; active
  names are deduplicated in insertion order. Proof:
  `bash_tool_exposes_live_session_environment_variables`.
- Events: `_handleAgentEvent`, persistence on `message_end` (a failed `appendMessage`/custom/bash append is an uncaught throw, matching the source's handler with no try/catch; proof `tests/agent_session.rs::persist_failures_are_uncaught_like_the_typescript_event_handler`), queue removal,
  duplicate-turn guard, `agent_end.willRetry`, extension event emission,
  `agent_settled`, runtime message markers (`AgentSessionEvent::Message`).
- `dispose`: aborts retry/compaction/summary/branch/bash and the agent, clears the
  thinking-phase deadline, invalidates the outgoing extension runner with the exact
  stale-ctx message, stops runtime-mailbox polling/wake, disconnects the agent
  subscription, clears listeners, then runs `cleanupSessionResources(sessionId)`.
  Cleanup failures surface as `AggregateError` (throw_uncaught, since `dispose` is
  infallible in Rust). Proof:
  `agent_session_runtime.rs::new_session_invalidates_the_outgoing_extension_context`,
  `new_session_cleans_up_the_outgoing_session_resources`.
- Prompting: `prompt`, `_promptTurn`, input hook, skill/template expansion, streaming
  queue errors, extension commands, `steer`, `followUp`, `continue`, `interrupt`,
  `abort`, `sendCustomMessage`, `sendUserMessage`, `clearQueue`, reservations.
- Models/thinking: `setModel`, `cycleModel`, scoped models, `setThinkingLevel`
  clamping, `cycleThinkingLevel`, queue modes, extension `model_select` /
  `thinking_level_select`, control-db model/thinking persistence. Control-db
  write failures propagate (`set_model`, `cycle_model`, `set_thinking_level`,
  `cycle_thinking_level`, quota fallback) as the source throws; the approval
  reviewer cannot reject through its `Option`-returning hook, so a goal-read
  error blocks the tool call (fail closed, recorded in `session-manager.md`).
- Retry: `_isRetryableError`, quota fallback, `_prepareRetry`, `auto_retry_*` events,
  `notifyProviderRetry`.
- Compaction: `compact`, `_checkCompaction`, `_runAutoCompaction`, `appendCompaction`,
  context estimate, materialization, abort hooks, compaction extension overrides.
- Bash: `executeBash` over `bash_executor` (shell prefix, streaming
  `bash_execution_update`, abort tokens), `recordBashResult`, pending flush,
  `abortBash`. `detachRunningTool`/`detachBashTool` and
  `hasDetachableTool`/`hasDetachableBashTool` delegate to the session's
  `ToolDetachRegistry` (`detach_running`/`has_running`); detaching does not
  unregister the handle, matching the source where the tool's subscription
  unregisters when its execution ends.
- Permissions: tool gates, auto-approve reviewers, approval orchestrator, MCP
  permission-prompt protocol tool (discovery/config, rule-store skip, updated input
  write-back), human UI reviewer (including the `Allow always` background
  `permission-config` spawn), supervisor LLM reviewer with read-only
  `approvalKind` auto-approval. Reviewer, UI-dialog, and Supervisor request
  failures propagate through `ApprovalReviewer` and become error tool results via
  the agent loop's `prepareToolCall` catch, matching the source instead of
  allowing or blocking the call. Proof:
  `human_allow_always_spawns_the_permission_config_agent`,
  `llm_reviewer_auto_approves_read_only_builtin_tools`,
  `llm_reviewer_does_not_auto_approve_extension_tools_reusing_a_read_only_name`,
  `human_approval_ui_errors_abort_the_tool_call`,
  `supervisor_requester_errors_abort_the_tool_call`.
- Session: names, compaction summary edits, `navigateTree` (no-summary and
  summarization paths with `session_before_tree`/`session_tree` and branch-summary
  attachment), fork selector, stats, context usage, `exportToJsonl`, `exportToHtml`,
  `getLastAssistantText`, `createReplacedSessionContext`.
- Frozen `agent-session.ts:3598-3605` releases the mailbox drain immediately
  before `_promptTurn`. Native passes ownership of the drain guard through
  claimed delivery and drops it in `prompt_mailbox_messages` before `prompt_turn`.
  Acceptance remains in prompt preparation; receipt persistence, canonical-row
  deletion, and finally-style unaccepted-claim release are unchanged. Proof:
  `agent_session_runtime_mailbox::idle_mailbox_prompt_completes_tool_result_and_next_assistant_turn`
  runs idle delivery -> faux read call -> successful tool result -> next assistant,
  with inbound coordination enabled and a five-second Tokio timeout. Retaining
  the guard times out after the successful read result (RED and explicit mutant).
- Runtime coordination: `drainRuntimeCoordination`, mailbox delivery at prompt /
  post-agent-run / after-tool-result checkpoints (steer vs prompt), extension
  `runtime_mailbox` interception, detached tool-call completion entries, shared-channel
  formatting/cursor delivery, listener registration on first drain.
- Multi-agent: activity publishing for `agent_start`, `tool_execution_start/end`, and
  `agent_end` with exact runtime ownership; relocation continuation via
  `AgentEndSessionContinuation::CwdRelocation` and `activateToolResultRelocation`.
  Publication failures propagate out of the agent event listener and fail the run,
  matching the source throw boundary
  (`activity_publication_errors_fail_the_agent_run`).
- Thinking-phase deadline: the 15-minute timer is armed on `agent_start` and after the
  last tool result, cleared on tool start/`agent_end`/abort/dispose, restarted by idle
  steering, skipped for observer runtimes and non-positive timeouts, and the recorded
  timeout error replaces the aborted run error. Proof:
  `thinking_phase_deadline_caps_main_session_thinking_but_not_tools`,
  `thinking_phase_deadline_reports_the_child_subject`,
  `thinking_phase_deadline_skips_observer_sessions`.
- Runtime mailbox timers and steering: the 30s polling timer, 60s listener heartbeat,
  SIGUSR2 signal wake with the keepalive guard, dynamic metadata control-db
  resolution, steering delivery acknowledgement, and runtime-mailbox steering turn
  finalization (including deferred terminal results for active descendants).
  Proof: `agent_session_runtime_mailbox` (5), `agent_session_signal_wake` (1).
- Background compaction: speculative cache generated during tool turns at 70% of the
  context window, reused by the next threshold compaction, invalidated on session/model/
  prompt/settings drift. The cache snapshots branch entries and the extension runner
  and honors the `compaction` hook before the local summarizer.
- Compaction extension boundary: `session_before_compact` preflight and the
  `compaction` provider event for manual and automatic compaction, cancel handling,
  extension-provided results when built-in compaction is disabled, and `fromHook` on
  every compaction entry. Proof: `agent_session_compaction_extensions` (9).
- Extensions: runner construction, `bind_core` actions/context/provider actions,
  `bind_extensions`, tool gates/reviewers/tool_call/tool_result hooks,
  `before_agent_start`, `input`, resources discovery, internal `model` command,
  `bind_session_runtime_relocation` + `AgentSessionRuntime::bind_session_runtime_actions`.
- Detached jobs: `bind_detached_job_lifecycle` provider from `MultiAgentStore`,
  terminal-outbox projection delivery on prompt.

## Definition carriers

- `AgentSessionConfig.base_tool_definitions` / `custom_tool_definitions` carry extension-style tool definitions
  that the session wraps with its live `ExtensionRunner` context (source info `<builtin:name>` / `<sdk:name>`).
  They preserve prompt snippets/guidelines and let coordination tools run with the runner context; `base_tools`
  remains the AgentTool seam used by tests.

## Gaps (explicit)

- None.

## Deviations

- `set_sandbox_profile` reads the status color through `try_current_theme()` and
  returns `Theme not initialized. Call initTheme() first.` as a session error;
  the source's `ctx.ui.theme` throws and the caller reports it.

## Discrepancies

- `AgentSession::compact` returns `core::compaction::CompactionResult` instead of
  `unknown`; typed API excludes the JS loose shape (approved typed-core rule).
- Runtime coordination `hasConfiguredAuth` uses the model-runtime snapshot, matching
  the source; a runtime created with `refreshOnCreate: false` must refresh before
  mailbox prompt delivery.
- The `Allow always` spawn failure path logs `Failed to spawn allow-always agent:`
  plus the tool error message; the source's `console.error` also prints the JS
  error stack.
- Differential `differential_agent_session` compares provider usage
  (`input`/`output`/`cacheRead`/`cacheWrite`/`totalTokens`), context usage
  `tokens`/`percent`, events, messages, stats, fork selector, navigation, and
  session/export JSONL byte-for-byte with no numeric normalization. Both sides run
  one active `end_turn` tool and a fixed custom system prompt
  (`"Differential system prompt"`). The oracle's stream adapter removes the
  TS-only `AgentTool` fields `label` and `executionMode` before handing the
  context to the faux provider, matching the provider-facing `Tool` shape Rust
  sends; real provider adapters drop those fields too.
- `getUserMessagesForForking` returns typed `UserMessageForForking` objects matching
  the TS `{entryId,text}` wire shape.

## v15 session names (approved 2026-10-20)

Name methods follow `c248b0067` ([reference lock](../source-lock.json)).
`set_session_name`/`clear_session_name` call the shared manager and propagate persistence errors.
Both publish `SessionInfoChanged` to subscribers and extensions; neither appends JSONL.
Proof: `agent_session::session_name_round_trips_through_session_metadata_state`,
`rpc_mode_session_name` (real RPC process; metadata value and no session-info entry),
`session_autoname` (explicit-clear gating and exact extension set/clear event ordering). The broad pinned AgentSession/RPC differentials
retain name commands/events but capture JSONL before renaming; v15 persistence has separate proof.

## JS-value boundary regression (2026-10-20)

Shared JS stringify/object enumeration is used at this module's audited observable boundary; values and parser errors are unchanged. Concrete counterexamples and per-boundary tests: [JS-value proof](../js-value-semantics-proof.md).

`export_to_jsonl` also uses shared stringify for header and linearized entries, matching pinned `agent-session.ts:5999,6005`. `jsonl_export_preserves_javascript_value_bytes` writes real JSONL with numeric keys, float-backed integers and an unsafe integer; only serialized bytes change.

## Q18 native consumer follow-up
Control DB, model/runtime and session-manager errors propagate message authorities rather than Display encoding. Native skill/template names and paths flow through slash commands and skill invocation. Skill expansion now follows frozen `_expandSkillCommand`: parse/read failure emits `skill_expansion` and preserves the original command; the old Rust parser-to-original-file fallback was an implementation discrepancy, not a frozen-source behavior. `invalid_skill_frontmatter_emits_native_path_error_and_keeps_original_command` was added before replacing the fallback; `skill-expansion-red.jsonl` remains library-compiler-blocked. Runtime acceptance not claimed.
