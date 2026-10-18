# agent-session.ts traceability

Rust: `crates/pi-coding-agent/src/core/agent_session.rs` (module `core::agent_session`).

## Exported symbols

| TS | Rust | Tests |
| --- | --- | --- |
| `AgentSession` | `AgentSession` | `tests/agent_session.rs` (27), `tests/differential_agent_session.rs` |
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
  write-back), human UI reviewer, supervisor LLM reviewer.
- Session: names, compaction summary edits, `navigateTree` (no-summary path),
  fork selector, stats, context usage, `exportToJsonl`, `exportToHtml`,
  `getLastAssistantText`, `createReplacedSessionContext`.
- Extensions: runner construction, `bind_core` actions/context/provider actions,
  `bind_extensions`, tool gates/reviewers/tool_call/tool_result hooks,
  `before_agent_start`, `input`, resources discovery, internal `model` command.
- Detached jobs: `bind_detached_job_lifecycle` provider from `MultiAgentStore`,
  terminal-outbox projection delivery on prompt.

## Gaps (explicit)

- Runtime mailbox, shared-channel delivery/formatting, `wait_agent` wakeups:
  multi-agent store slice.
- `BashToolOptions` spawn hook / session environment / detach registry / background
  jobs wiring: owned by the tool-definition factory slice (`createAllToolDefinitions`).
- `navigateTree` summarization: `session_before_tree` events, branch summary
  generation/attachment, `session_tree` emission. No-summary navigation is ported.
- Background compaction cache, provider-native compaction source hints: local
  compaction only.
- `spawn_allow_always` agent for the human "Allow always" choice (the selection is
  accepted without spawning the config agent).
- Read-only `approvalKind` auto-approval in the LLM reviewer: session tool definitions
  do not carry `approvalKind`.
- `_startThinkingPhaseDeadline` / child activity publication and duplicate-turn stop
  hook installation: partial (duplicate guard exists; thinking deadline timers and
  multi-agent activity publishing are absent).
- Multi-agent agent/parent identity config is stored but activity/mailbox behavior is
  not implemented.

## Discrepancies

- `AgentSession::compact` returns `core::compaction::CompactionResult` instead of
  `unknown`; typed API excludes the JS loose shape (approved typed-core rule).
- Differential normalizes provider usage-derived numbers (message usage and context
  usage `tokens`/`percent`) because the Rust differential session uses a reduced tool
  set/system prompt; event order, deltas, messages, session JSONL, fork selector,
  navigation, and export JSONL are compared exactly.
- `getUserMessagesForForking` returns typed `UserMessageForForking` objects matching
  the TS `{entryId,text}` wire shape.
