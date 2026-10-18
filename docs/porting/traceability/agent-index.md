# Agent package index traceability

Pinned source: `packages/agent/src/index.ts` at
`532f7d12cd19235081bd4d6e425bde185652d86f`. Rust: `crates/pi-agent-core/src/lib.rs`.

The crate root mirrors the package entrypoint: runtime values and records are
re-exported directly, and TypeScript-only inference helpers or JavaScript
idioms map to the documented Rust replacements.

| TypeScript source | Observable contract | Rust export |
|---|---|---|
| `index.ts:3` | Re-export `uuidv7` | `pi_ai::utils::uuid::uuidv7` |
| `index.ts:4-31` | Re-export telemetry types from `@earendil-works/pi-telemetry` | `pi_telemetry` runtime/schema types re-exported from `lib.rs` (`AttributeValue`, `SpanAttributes`, `SpanOptions`, `SpanStatus`, `TelemetryContext`, `TelemetrySpan`, `TelemetrySchemaDefinition`, `TelemetrySpanDefinition`, `TelemetryAttributeDefinition`, `TelemetryAttributeMetadata`, `TelemetryAttributeType`, `TelemetryEventAttributeDefinition`, `TelemetryEventDefinition`, `TelemetryParentDefinition`, `TelemetryStartAttributeDefinition`, `RecordedTelemetryEvent`, `RecordedTelemetrySpan`, `TypedSpanStarter`) |
| `index.ts:32-37` | Re-export `createTypedSpanStarter`, `defineTelemetrySchema`, `InMemoryTelemetryContext`, `NOOP_TELEMETRY_CONTEXT` | Same names from `pi_telemetry` |
| `index.ts:38` | `export * from "./agent.ts"` | `Agent`, `AgentInitialState`, `AgentListener`, `AgentOptions`, `ContinueOptions`, `PrepareNextTurnSignalHook`, `PromptInput` |
| `index.ts:40` | `export * from "./agent-loop.ts"` | `AgentEventSink`, `AgentEventStream`, `ScriptedAssistantStream`, `agent_loop`, `agent_loop_continue`, `run_agent_loop`, `run_agent_loop_continue` |
| `index.ts:41` | `export * from "./harness/agent-harness.ts"` | `harness::agent_harness::*`; see `agent-harness.md` |
| `index.ts:42-53` | Branch-summary subset | `BranchPreparation`, `BranchSummaryDetails`, `BranchSummaryResult`, `CollectEntriesResult`, `FileOperations`, `GenerateBranchSummaryOptions`, `collect_entries_for_branch_summary`, `generate_branch_summary`, `prepare_branch_entries` |
| `index.ts:54-72` | Compaction subset | `CompactionPreparation`, `CompactionSettings`, `CompactResult`, `calculate_context_tokens`, `compact`, `DEFAULT_COMPACTION_SETTINGS`, `estimate_context_tokens`, `estimate_tokens`, `find_cut_point`, `find_turn_start_index`, `generate_summary`, `generate_summary_with_usage`, `get_last_assistant_usage`, `prepare_compaction`, `serialize_conversation`, `should_compact` |
| `index.ts:73` | `export * from "./harness/messages.ts"` | `harness::messages::*` |
| `index.ts:74` | `export * from "./harness/prompt-templates.ts"` | `harness::prompt_templates::*` |
| `index.ts:76` | `export * from "./harness/result.ts"` | Rust `Result`/`Ok`/`Err` idioms; see `result.md` |
| `index.ts:77` | `export * from "./harness/session/index.ts"` | `harness::session::*` |
| `index.ts:78` | `export * from "./harness/skills.ts"` | `harness::skills::*` |
| `index.ts:79` | `export * from "./harness/system-prompt.ts"` | `format_skills_for_system_prompt` |
| `index.ts:80-95` | Harness telemetry type subset | `AiSpan`, `AiSpanAttributes`, `AiSpanEndAttributes`, `AiSpanName`, `AiSpanStartAttributes`, `AiTelemetrySpan`, `HarnessSpan`, `HarnessSpanAttributes`, `HarnessSpanEndAttributes`, `HarnessSpanName`, `HarnessSpanStartAttributes`, `HarnessTelemetrySpan` |
| `index.ts:96-103` | Harness telemetry values | `AGENT_TELEMETRY_SCHEMAS` equivalent `agent_telemetry_schemas()`, `ai_telemetry_schema()`, `harness_telemetry_schema()`, `start_ai_span`, `start_harness_span` |
| `index.ts:104` | `export * from "./harness/tools/index.ts"` | `BashExecution`, `BashPrepare`, `BashTool`, `BashToolOptions`, `EditTool`, `ExecutionToolContext`, `ImageProcessorOptions`, `ReadImageProcessor`, `ReadImageProcessorResult`, `ReadTool`, `ReadToolOptions`, `WriteTool` |
| `index.ts:105-136` | Harness type/error subset | `AgentHarnessResources`, `AgentHarnessStreamOptions`, `AgentHarnessStreamOptionsPatch`, `AgentHarnessTool`, `AgentHarnessToolContextSource`, `AgentToolExecutionContext`, `BeforeToolCallResult`, `BranchSummaryError`, `BranchSummaryErrorCode`, `CompactionError`, `CompactionErrorCode`, `ExecutionEnv`, `ExecutionError`, `ExecutionErrorCode`, `FileError`, `FileErrorCode`, `FileInfo`, `FileKind`, `FileSystem`, `PromptTemplate`, `QueueMode`, `Shell`, `ShellExecOptions`, `Skill`, `ThinkingLevel`, `ToolExecutionMode` |
| `index.ts:137` | `export * from "./harness/utils/shell-output.ts"` | `ShellCaptureOptions`, `ShellCaptureProgress`, `ShellCaptureResult`, `ShellChunkHandler`, `execute_shell_with_capture`, `sanitize_binary_output` |
| `index.ts:138` | `export * from "./harness/utils/truncate.ts"` | `DEFAULT_MAX_BYTES`, `DEFAULT_MAX_LINES`, `GREP_MAX_LINE_LENGTH`, `TruncatedBy`, `TruncatedLine`, `TruncationOptions`, `TruncationResult`, `format_size`, `truncate_head`, `truncate_line`, `truncate_tail` |
| `index.ts:140` | `export * from "./proxy.ts"` | `ProxyAssistantMessageEvent`, `ProxyStreamOptions`, `stream_proxy` |
| `index.ts:141` | `export * from "./search/index.ts"` | `SessionSearch`, `SessionSearchHit`, `SessionSearchOptions` |
| `index.ts:143` | `export { setDefaultStreamFn }` | `set_default_stream_fn` |
| `index.ts:145` | `export * from "./types.ts"` | `agent::types::*` (`AgentMessage`, `AgentTool`, `AgentEvent`, loop hooks/context, and related records) |

## Gaps / discrepancies

- `ok`, `err`, `getOrThrow`, `getOrUndefined`, `toError`, and the `Result`
  namespace object are JavaScript idioms; Rust uses `Result`, `Ok`, `Err`, `?`,
  and `Option`. See `result.md` and `harness-types.md`.
- Telemetry conditional-type helpers (`ExactTelemetryAttributes`, `Infer*`,
  `SchemaTelemetrySpan`, `TelemetrySchemaSpan*`) have no Rust equivalent. See
  `harness-telemetry.md` and `telemetry.md`.
- `AGENT_TELEMETRY_SCHEMAS`, `AI_TELEMETRY_SCHEMA`, and
  `HARNESS_TELEMETRY_SCHEMA` are exposed as functions because populated maps
  cannot be built in a Rust constant.
