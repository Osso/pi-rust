# extensions/types.ts → `core::extensions::types`

Source: `packages/coding-agent/src/core/extensions/types.ts` (2001 lines).
Rust: `crates/pi-coding-agent/src/core/extensions/types.rs`.

Approved deviation: TypeScript extension files are not loaded (AGENTS.md). The
event/API contracts are Rust-native; factories receive `ExtensionApi`.

## Contract adaptations

- Deviation (approved by the parent slice): handlers receive owned
  `ExtensionEvent` payloads and an owned `ExtensionContext` snapshot. Async
  event mutation is expressed through results: `ToolCallEventResult.input`,
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
- `Theme` is reduced to `ExtensionTheme::fg`, the only theme operation used by
  the runner.

## Symbols → Rust

| Source | Rust | Tests |
| --- | --- | --- |
| `ExtensionUIDialogOptions` | `ExtensionUiDialogOptions` | `cancelled_context_signal_aborts_dispatch` |
| `WidgetPlacement`, `ExtensionWidgetOptions`, `WorkingIndicatorOptions` | same names | serialization via differential |
| `TerminalInputHandler` | `TerminalInputHandler`, `TerminalInputResult` | compile-time |
| `ExtensionUIContext` | `ExtensionUiContext` trait + `NoOpUiContext` | differential `shortcuts` (hasUI), runner tests |
| `ContextUsage`, `CompactOptions`, `ExtensionMode` | same names | differential |
| `ExtensionContext` | `ExtensionContext` + `ExtensionContextCallbacks` | runner/discovery tests, differential |
| `ExtensionCommandContext` | `ExtensionCommandContext` (Deref to context, method wrappers) | `command_context_passes_fork_options_through`, `stale_contexts_reject_actions` |
| `ReplacedSessionContext` | `ReplacedSessionContext` | compile-time |
| `ToolRenderResultOptions`, `ToolRenderContext` | same names (`last_component: Option<ComponentRef>`) | compile-time |
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
| `FlagValue`, `ExtensionFlag`, `FlagType` | same names | differential `flags` |
| `ExtensionShortcut`, `RegisteredTool`, `Extension` | same names; ordered `IndexMap`s | differential, discovery tests |
| `ExtensionHandlers` | typed handler vectors (one per event) | differential |
| `LoadExtensionsResult`, `ExtensionError` | same names | differential `errors`, discovery tests |
| `ProviderConfig`, `ProviderModelConfig`, `InlineExtension`, `ExtensionFactory` | aliases in `types`/`loader` | differential `providers` |

## Gaps

- Component-factory UI methods are not ported: `setWidget` factory overload,
  `setFooter`, `setDefaultFooter`, `setHeader`, `custom`,
  `addAutocompleteProvider`, `setEditorComponent`, `getEditorComponent`, and
  `theme`/`getTheme`/`setTheme(Theme)` object accessors. They depend on the
  interactive TUI/theme port. String-array widgets, status/working state,
  titles, editor text, theme listing and `setTheme(name)` are implemented.
- `AutocompleteProviderFactory`/`EditorFactory` aliases are omitted with the
  same dependency.
- `DetachedJobLifecycleController` is not exposed on the context
  (detached-job runner not ported).
- Tool input/detail types remain JSON objects until the tools slices land.

## Discrepancies

- `emitInput` compares images by value where TypeScript compares array identity
  (`currentImages !== images`). Scripted handlers that return the same array do
  not produce a transform in Rust when the contents are unchanged.
- Context getters snapshot values at `createContext()`; TypeScript getters read
  live runner fields. `signal` keeps token identity, so later cancellation is
  observed.
