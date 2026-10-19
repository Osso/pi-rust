# agent-session.ts traceability

Rust: `crates/pi-coding-agent/src/core/agent_session.rs` (module `core::agent_session`).

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
| `DETACHED_TOOL_CALL_COMPLETION_CUSTOM_TYPE`, `buildDetachedToolCallCompletionEntry` | gap | multi-agent store slice |
| `bindAgentSessionMutationTargetResolver` | gap | interactive slice |

## Behavior coverage

- Constructor: role/capability validation, tool registry, system prompt rebuild, hooks,
  extension runner construction, control-db path installation, permission rule store,
  supervisor requester default, detached-job lifecycle binding.
- Base tools: `createAllToolDefinitions` rebuilt on every `_buildRuntime` from current
  settings (read auto-resize, bash command prefix/shell path/detach registry/background
  jobs, live session environment); `baseToolsOverride` path appends `end_turn`; active
  names are deduplicated in insertion order. Proof:
  `bash_tool_exposes_live_session_environment_variables`.
- Events: `_handleAgentEvent`, persistence on `message_end`, queue removal,
  duplicate-turn guard, `agent_end.willRetry`, extension event emission,
  `agent_settled`, runtime message markers (`AgentSessionEvent::Message`).
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
  `abortBash`.
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
