# extensions/types.ts → `core::extensions::types`

Source: `packages/coding-agent/src/core/extensions/types.ts` (2001 lines).
Rust: `crates/pi-coding-agent/src/core/extensions/types.rs`.

Approved deviation: TypeScript extension files are not loaded (AGENTS.md). The
event/API contracts are Rust-native; factories receive `ExtensionApi`.

## Contract adaptations

- Deviation (approved by the parent slice): handlers receive owned
  `ExtensionEvent` payloads; `ExtensionContext` is a cheap handle to shared
  runner state, so every runner-backed member is a method that asserts the
  runner is active and reads current values. Async event mutation is expressed
  through results: `ToolCallEventResult.input`,
  `BeforeProviderHeadersEvent` returning `ProviderHeaders`,
  `ContextEventResult.messages`, `BeforeProviderRequestEventResult`,
  `InputEventResult`, and `ToolResultEventResult`. Observable behavior matches
  TypeScript in-place mutation: the runner applies each patch before invoking
  the next handler, so later handlers see earlier mutations, and the final
  input/headers reach tool execution and the provider request. The differential
  `mutations` scenario proves this against the real TypeScript runner with two
  chained handlers for `tool_call` input and `before_provider_headers`.
- Tool-call inputs are `serde_json::Map<String, Value>`. The TypeScript union
  carries per-tool input/detail types owned by the tools slices
  (`BashToolInput`, `ReadToolInput`, …, `BashToolDetails`, …); this module does
  not duplicate them. Type guards (`isToolCallEventType`, `is*ToolResult`) are
  preserved as functions.
- `UserBashEventResult.operations` carries a typed
  `Arc<dyn BashOperations>` instead of serialized JSON, so `user_bash`
  extensions can replace execution like the source.
- `ctx.sessionManager` is the owning `AgentSession`'s live
  `Arc<Mutex<SessionManager>>` (`SharedSessionManager`), not a snapshot: reads
  observe session-side mutations and goal writes persist through the same
  manager. Tests: `context_session_manager_is_the_live_shared_handle`,
  `session_replacement_rebinds_the_shared_extension_handle`.
- `CustomMessage` (`core/messages.ts`, owned by the session slice) is
  represented by `CustomMessagePayload` (`customType`/`content`/`display`/
  `details`), the exact `Pick<>` shape used by the API.
- `ProviderConfig` is `provider_composer::ProviderConfigInput`;
  `ProviderModelConfig` is `Model` (ca-models decision: Rust models carry their
  full shape). OAuth uses `ExtensionOAuthConfig`.
- `CompactionPreparation`, `CompactionResult`, and `CompactionSourceInfo` are
  temporary definitions here; ca-compaction-export owns their final home and
  will replace these without changing runner dispatch.
- `AbortSignal` maps to `tokio_util::sync::CancellationToken`.
- `Theme` is the concrete `modes::interactive::theme::Theme`; `ExtensionUIContext::theme()`
  returns the process theme (`Arc<Theme>`), matching the source `readonly theme: Theme`.

## Symbols → Rust

| Source | Rust | Tests |
| --- | --- | --- |
| `ExtensionUIDialogOptions` | `ExtensionUiDialogOptions` | `cancelled_context_signal_aborts_dispatch` |
| `WidgetPlacement`, `ExtensionWidgetOptions`, `WorkingIndicatorOptions` | same names | serialization via differential |
| `TerminalInputHandler` | `TerminalInputHandler`, `TerminalInputResult` | compile-time |
| `ExtensionUIContext` | `ExtensionUiContext` trait + `NoOpUiContext` | differential `shortcuts` (hasUI), runner tests |
| `ContextUsage`, `CompactOptions`, `ExtensionMode` | same names | differential |
| `ExtensionContext` | `ExtensionContext` + `ExtensionContextState`; runner-backed members are fallible methods reading live shared state | differential `context_live`, `context_members_read_live_runner_state` |
| `ExtensionCommandContext` | `ExtensionCommandContext` (Deref to context, live method wrappers) | `command_context_passes_fork_options_through`, `stale_context_reads_throw_the_typescript_error_text` |
| `ReplacedSessionContext` | `ReplacedSessionContext` | compile-time |
| `ToolRenderResultOptions`, `ToolRenderContext`, `ToolRenderState` | same names (`last_component: Option<ComponentRef>`, `state` is a shared `ToolRenderState` so renderers mutate one object per tool row) | `tool_renderer::shares_render_state_between_calls`, `edit_tool_preview` |
| `ToolRenderer`, `ToolResultRenderer` | fallible (`Result<ComponentRef, HandlerError>`); `infallible_render_call`/`infallible_render_result` adapt built-ins | `tool_execution_renderer_errors` |
| `ToolDefinition`, `defineTool` | `ToolDefinition`, `define_tool` | `keeps_the_first_registered_tool_and_definition` |
| `ProjectTrustEvent/Result/Context/Handler` | same names | `project_trust_collects_handler_errors_and_keeps_scanning`, differential `project_trust` |
| `ResourcesDiscoverEvent/Result` | same names | differential `dispatch` |
| Session events (`SessionStart` … `SessionTree`) | same names; `SessionEntry`/`CompactionEntry`/`BranchSummaryEntry` reused | differential `session_events` |
| `RuntimeMailboxEvent` | same name (`RuntimeMailboxMessage` reused) | differential `mailbox` |
| Agent events (`Context` … `ToolExecutionEnd`) | same names (`AgentMessage`, `AssistantMessageEvent`, `ToolResultMessage` reused) | differential `dispatch`, `errors` |
| `ModelSelectEvent`, `ThinkingLevelSelectEvent` | same names (`ThinkingLevel` from agent-core) | compile-time |
| `UserBashEvent/Result` | same names; operations/result are JSON until the bash tool port | differential `dispatch` |
| `InputEvent`, `InputEventResult`, `InputSource` | same names | differential `input` |
| `ToolCallEvent`, `ToolResultEvent`, unions, guards | single structs + `ToolName` + guards | differential `dispatch`, `errors`; `gates_and_reviewers_patch_tool_input` |
| Result records (`ToolCallEventResult` … `SessionBeforeTreeResult`) | same names; `ToolCallEventResult.input` is the Rust patch channel | differential |
| Renderers (`MessageRenderer`, `EntryRenderer`, `MarkdownTransformer`) | same aliases returning `ComponentRef` | `collects_renderers_and_transformers_in_load_order` |
| `RegisteredCommand`, `ResolvedCommand` | same names (flat `ResolvedCommand`) | differential `commands` |
| `ExtensionAPI` | `ExtensionApi` struct, one `on_*` method per event | differential, runner tests |
| `ExtensionContext.setThinkingLevel` | `ExtensionContext::set_thinking_level` bound from `ExtensionContextActions::set_thinking_level` | `extensions.rs::context_set_thinking_level_invokes_the_session_setter`; effort/codex-fast differential |
| `FlagValue`, `ExtensionFlag`, `FlagType` | same names | differential `flags` |
| `ExtensionShortcut`, `RegisteredTool`, `Extension` | same names; ordered `IndexMap`s | differential, discovery tests |
| `ExtensionHandlers` | typed handler vectors (one per event) | differential |
| `LoadExtensionsResult`, `ExtensionError` | same names | differential `errors`, discovery tests |
| `ProviderConfig`, `ProviderModelConfig`, `InlineExtension`, `ExtensionFactory` | aliases in `types`/`loader` | differential `providers` |

## Gaps

- Component-factory UI contracts are defined in `types` (`ExtensionUiComponent`,
  `ExtensionUiTui`, factory contexts, `setWidget` factory overload, `setFooter`,
  `setDefaultFooter`, `setHeader`, `custom`, `getTheme`) but the interactive
  implementation lands with `port/ca-interactive-mode`. `theme()`, string-array
  widgets, status/working state, titles, editor text, theme listing and
  `setTheme(name)` are implemented.
- `addAutocompleteProvider`, `setEditorComponent`, `getEditorComponent`, and the
  `setTheme(Theme)` object accessor remain gaps.
- `AutocompleteProviderFactory`/`EditorFactory` aliases are omitted with the
  same dependency.
- `DetachedJobLifecycleController` is exposed as `ExtensionContext::detached_job_lifecycle()`
  and bound by `ExtensionRunner::bind_detached_job_lifecycle`. `core::agent_session`
  (owned by ca-agent-session-2) must call that binder with the session controller.
- Tool input/detail types remain JSON objects until the tools slices land.

## Discrepancies

- `emitInput` compares images by value where TypeScript compares array identity
  (`currentImages !== images`). Scripted handlers that return the same array do
  not produce a transform in Rust when the contents are unchanged.

## Representation

- TypeScript returns getters that call `runner.assertActive()` and read current
  runner fields. Rust exposes the same members as methods on `ExtensionContext`
  backed by `ExtensionContextState`; each method performs the stale check and
  reads live state. `toolExecutionStartedAt`/`toolExecutionAgentId` remain
  per-invocation fields.
