# agent-session.ts traceability

Rust: `crates/pi-coding-agent/src/core/agent_session.rs` (module `core::agent_session`).

## Exported symbols

| TS | Rust | Tests |
| --- | --- | --- |
| `AgentSession` | `AgentSession` | `tests/agent_session.rs` (15), `tests/differential_agent_session.rs` |
| `AgentSessionConfig` | `AgentSessionConfig` | same |
| `AgentSessionEvent` | `AgentSessionEvent` (public) | event assertions + differential |
| `AgentSessionEventListener` | `AgentSessionEventListener` | — |
| `PromptOptions` | `PromptOptions` | prompt/steer tests |
| `ModelCycleResult` | `ModelCycleResult` | cycle tests |
| `SessionStats` | `SessionStats` (public) | stats test, differential |
| `ExtensionBindings` | `ExtensionBindings` + `bind_extensions` | compile-level; no behavioral test (needs UI host) |
| `MultiAgentRuntimeRole` / `createMultiAgentExecutionCapability` | same | constructor validation |
| `getAssistantMessageText` | `get_assistant_message_text` | exercised by duplicate/stat paths |
| `shouldContinueInterruptedSession` | `should_continue_interrupted_session` | unit helper, no dedicated test |
| `DETACHED_TOOL_CALL_COMPLETION_CUSTOM_TYPE`, `buildDetachedToolCallCompletionEntry` | gap | multi-agent store slice |
| `bindAgentSessionMutationTargetResolver` | gap | interactive slice |

## Behavior coverage

- Constructor: role/capability validation, tool registry, system prompt rebuild, hooks, extension runner construction, mailbox polling (gap).
- Events: `_handleAgentEvent`, persistence on `message_end`, queue removal, duplicate-turn guard, `agent_end.willRetry`, extension event emission, `agent_settled`.
- Prompting: `prompt`, `_promptTurn`, input hook, skill/template expansion, streaming queue errors, extension commands, `steer`, `followUp`, `continue`, `interrupt`, `abort`, `sendCustomMessage`, `sendUserMessage`, `clearQueue`, reservations.
- Models/thinking: `setModel`, `cycleModel`, scoped models, `setThinkingLevel` clamping, `cycleThinkingLevel`, queue modes, extension `model_select`/`thinking_level_select`.
- Retry: `_isRetryableError`, quota fallback, `_prepareRetry` (abortable sleep), `auto_retry_*` events, `notifyProviderRetry`.
- Compaction: `compact`, `_checkCompaction` (overflow/threshold), `_runAutoCompaction`, `appendCompaction`, context estimate, `materializeCompactionSummary`, abort hooks. No dedicated test yet; manual path is exercised only through compile/type checks.
- Bash: `recordBashResult`, pending flush, `abortBash`; `executeBash` itself is owned by the bash-executor slice (gap).
- Session: `setSessionName`, `clearSessionName`, `updateCompactionSummary`, `navigateTree` (no-summary path), `getUserMessagesForForking`, stats, context usage, `exportToJsonl`, `getLastAssistantText`.
- Extensions: runner construction, `bind_core` actions/context/provider actions, `bind_extensions`, tool gates/reviewers/tool_call/tool_result hooks, `before_agent_start`, `input`, resources discovery, internal `model` command.

## Gaps (explicit)

- `_startRuntimeMailbox*`, `_drainRuntimeCoordination*`, shared-channel delivery, terminal outbox, detached job lifecycle/cleanup, `MultiAgentStore` activity publication: owned by control-db / multi-agent slices.
- Permission prompt protocol tool (`permissions/mcp-permission-prompt.ts`), supervisor LLM/human reviewers, `spawn_allow_always` agent: unported modules.
- Control-db metadata writes (`setSessionModel`, `setSessionThinkingLevel`, `readPersistedSessionSettings`, `setMetadataControlDbPath`): `SessionManager` metadata integration still blocked on ca-control-db follow-up.
- `DefaultResourceLoader` / `createAllToolDefinitions`: callers must inject `ResourceLoader` and `base_tools`; `sdk` documents this.
- Background compaction cache, extension `session_before_compact`/`compaction` result overrides, provider-native compaction source hints: extension compaction result types differ; local compaction only.
- `createReplacedSessionContext`, `hasExtensionHandlers` returns false until extension contexts are wired by the runtime host.
- Image normalization (`utils/tool-result-images.ts`) in `afterToolCall`.
- Runtime message markers (`runtime-message-markers.ts`): owned by ca-cli.
- `exportToHtml`: `export_html` module exists but theme/tool-renderer wiring is not connected here.

## Discrepancies

- `AgentSession::compact` returns `core::compaction::CompactionResult` instead of `unknown`; typed API excludes the JS loose shape (approved typed-core rule).
- Event summary used by the differential omits `message_update` frames because faux stream delta chunking is provider-internal; all lifecycle frames and message payloads are compared.
- `sdk` errors when no model is available because Rust `Agent` requires a `Model`; TS carries `undefined` through. Typed-core exclusion.
