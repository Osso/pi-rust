# extensions/types.ts → `core::extensions::types`

Frozen 4f9e6f8a4 model_request_start/model_request_end payloads and registration/dispatch were added with the foreground wait slice. AgentSession forwards both real AgentEvent variants; focused runner+wait deadline proof is in [wait contract](../../specs/agent-wait-slice.md). Whole extension event/API re-pin coverage remains independent.

Source: `packages/coding-agent/src/core/extensions/types.ts` (2001 lines).
Rust: `crates/pi-coding-agent/src/core/extensions/types.rs`.

Approved deviation: TypeScript extension files are not loaded (AGENTS.md). The
event/API contracts are Rust-native; factories receive `ExtensionApi`.

## Native terminal/indicator checkpoint

`TerminalInputHandler` receives `&JsString`, `TerminalInputResult.data` is `Option<JsString>`, and `WorkingIndicatorOptions.frames` is `Option<Vec<JsString>>`. No scalar terminal adapter remains in those contracts. `raw_extension_ui_types.rs` tests lone high/low units and astral callback values plus lossless frame serialization; execution is pending whole coding integration compilation. Interactive dispatch is a separate native migration, not proof supplied by this contract change.

## Native custom-component checkpoint

`ExtensionUiComponent::render` returns `Vec<JsString>` and `handle_input` takes `&JsString`. Both synchronous and asynchronous `ExtensionUiCustomResult` error carriers are `JsString`. The question panel uses this single native interface. Mechanical signature updates cover the 19 extension-example implementers and LlamaViewComponent; native propagation inside their existing bodies is not claimed complete. Interactive implementers remain owned by the interactive lane. No scalar component adapter is introduced; actual coding-library proof remains pending compilation.

## Contract adaptations

- Representation deviation (Rust-native extension API, AGENTS.md): handlers
  receive owned `ExtensionEvent` payloads; `ExtensionContext` is a cheap handle to shared
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
| `ToolRenderer`, `ToolResultRenderer` | fallible (`Result<ComponentRef, HandlerError>`); `infallible_render_call`/`infallible_render_result` adapt built-ins | `tool_execution_renderer_errors::failing_call_and_result_renderers_use_the_typescript_fallbacks` |
| `ToolDefinition`, `defineTool` | `ToolDefinition`, `define_tool` | `keeps_the_first_registered_tool_and_definition` |
| `ProjectTrustEvent/Result/Context/Handler` | same names | `project_trust_collects_handler_errors_and_keeps_scanning`, differential `project_trust` |
| `ResourcesDiscoverEvent/Result` | same names | differential `dispatch` |
| Session events (`SessionStart` … `SessionTree`) | same names; `SessionEntry`/`CompactionEntry`/`BranchSummaryEntry` reused | differential `session_events` |
| `RuntimeMailboxEvent` | same name (`RuntimeMailboxMessage` reused) | differential `mailbox` |
| Agent events (`Context` … `ToolExecutionEnd`) | same names (`AgentMessage`, `AssistantMessageEvent`, `ToolResultMessage` reused) | differential `dispatch`, `errors` |
| `ModelSelectEvent`, `ThinkingLevelSelectEvent` | same names (`ThinkingLevel` from agent-core) | compile-time |
| `UserBashEvent/Result` | same names; operations/result are JSON until the bash tool port | differential `dispatch` |
| `InputEvent`, `InputEventResult`, `InputSource` | same names; `InputImages` carries array identity | differential `input`, `input_images` |
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

None.

## Discrepancies

- `emitInput` image identity is modeled by `InputImages` (an `Arc<Vec<ImageContent>>`
  with `ptr_eq`). TypeScript compares `currentImages !== images`; cloning the
  event images preserves the reference and yields `continue`, while a freshly
  built `InputImages` with equal contents yields `transform`. In-place mutation
  of the shared array is not expressible through the Rust contract (contents
  are immutable); a Rust handler returns a new value instead, which produces
  `transform` rather than TypeScript's identity-preserving `continue`. That is
  an API-shape deviation, not a dropped observable result: the transformed
  value still reaches the prompt. Evidence: differential `input_images` in
  `tests/extensions.rs` runs both cases against the pinned TS runner.

## Representation

- TypeScript returns getters that call `runner.assertActive()` and read current
  runner fields. Rust exposes the same members as methods on `ExtensionContext`
  backed by `ExtensionContextState`; each method performs the stale check and
  reads live state. `toolExecutionStartedAt`/`toolExecutionAgentId` remain
  per-invocation fields.

## Provider response header order

`AfterProviderResponseEvent.headers` is `IndexMap<String, String>`, forwarded unchanged from `ProviderResponse.headers` by SDK hooks. `tests/sdk.rs::custom_provider_response_preserves_header_order_through_sdk_and_json` registers a custom provider through `ExtensionApi`, invokes its response callback, and asserts z-first/a-second handler and serialized event order (the pinned TS JSON.stringify contract).

## Editor host and optional focus

`ExtensionUiTui` implements `EditorHost`, including live `create_fixed_cell` on the owning UI thread. `ExtensionUiComponent::focused() -> Option<bool>` models TS property presence; `set_focused` forwards focus changes only for opted-in components. Proof: interactive unit tests `extension_ui_factory_builds_real_custom_editor`, `extension_ui_overlay_focus_emits_cursor_and_restores_editor`.

`ExtensionUiComponent::width` models the optional component width used by the pinned custom-overlay host. Explicit overlay options map to TUI options without losing visibility/sizing fields. Tests: `extension_ui_overlay_uses_component_width_and_closes_once`, `extension_ui_overlay_explicit_options_and_synchronous_close`.

## Q18 native local commands/completions (2026-10-22)

RegisteredCommand/ResolvedCommand name, invocation_name and description are
JsString; the command map key and register/call-command actions use that same
identity. Local slash-command names are not the provider-ID scalar exception.
UserBashEvent.cwd is native.

ArgumentCompletions is one API: Fn(&JsString) -> Result<Option<Vec<JsValue>>,
JsString>. Run-plan/safe/codex-usage and example producers retain native prefix/
results and propagate errors; direct literal descriptions/maps and fixtures are
adapted mechanically. The interactive adapter preserves Err and rejects malformed
completion items explicitly instead of dropping them.204 owns TUI future/error
propagation to uncaught rejection; source tui/autocomplete.ts:353 and
components/editor.ts:2531 contain no catch.

Corrected discrepancy: the former Option-only callback representation could not
carry source thrown errors; run-plan also discarded cwd/readdir failures. Both
are replaced with the Result channel, not a new fallback or a second API. A real
extension registration fixture asserts raw command lookup and raw callback
failure; written but unexecuted while coding compilation is blocked.
