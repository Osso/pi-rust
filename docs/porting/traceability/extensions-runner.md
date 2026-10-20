# extensions/runner.ts → `core::extensions::runner`

Source: `packages/coding-agent/src/core/extensions/runner.ts` (1552 lines).
Rust: `crates/pi-coding-agent/src/core/extensions/runner.rs`.

Differential: `tests/extensions.rs::extensions_match_the_pinned_typescript_runner`
drives the real TypeScript runner through 13 scripted scenarios (`dispatch`,
`errors`, `shortcuts`, `commands`, `flags`, `input`, `project_trust`,
`providers`, `mailbox`, `session_events`, `mutations`, `context_live`,
`provider_request`).

## Symbols → Rust

| Source | Rust | Tests |
| --- | --- | --- |
| `RESERVED_KEYBINDINGS_FOR_EXTENSION_CONFLICTS` | same constant | differential `shortcuts` |
| `buildBuiltinKeybindings` | `build_builtin_keybindings` | differential `shortcuts` |
| `BeforeAgentStartCombinedResult` | same name | differential `dispatch` |
| `callContextHandlerAbortable` | `call_context_handler_abortable` (`tokio::select!`) | `cancelled_context_signal_aborts_dispatch` |
| `ExtensionErrorListener` | same alias + `ErrorSubscription` | differential `errors` |
| handler aliases (`NewSessionHandler`, `ForkHandler`, …) | `NewSessionCallback`, `ForkCallback`, `NavigateTreeCallback`, `SwitchSessionCallback`, `ReloadHandler`-equivalent fields | `command_context_passes_fork_options_through` |
| `ProviderActions` | `ProviderActions` | differential `providers` |
| `emitSessionShutdownEvent` | `emit_session_shutdown_event` | `session_shutdown_helper_reports_handler_presence` |
| `emitProjectTrustEvent` | `emit_project_trust_event` | differential `project_trust`, `project_trust_collects_handler_errors_and_keeps_scanning` |
| `noOpUIContext` | `NoOpUiContext` | differential `dispatch` |
| `ExtensionRunner::constructor` | `ExtensionRunner::new` | all |
| `bindCore` | `bind_core` (&mut) | differential `providers`, runner tests |
| `bindRuntimeActions`, `bindContextActions` | same (private) | differential |
| `flushPendingProviderRegistrations` | same | differential `providers`, `reports_invalid_queued_provider_registration` |
| `bindImmediateProviderActions` | same | differential `providers` |
| `registerProvider`/`registerNativeProvider`/`unregisterProvider` | same; registry errors propagate | differential `providers` |
| `setRelocateHandler`, `setToolResultRelocationHandlers`, `deliverToolResultRelocation`, `hasPrepared…`, `activate…` | same | compile-time; relocation requires a session mode |
| `bindCommandContext` | same (defaults + bound actions) | `command_context_passes_fork_options_through` |
| `setUIContext`, `getUIContext`, `hasUI` | same (shared live state) | differential `shortcuts`, `context_live` |
| `getExtensionPaths` | same | discovery tests |
| `getAllRegisteredTools`, `getToolDefinition` | same (first name wins, load order) | `keeps_the_first_registered_tool_and_definition` |
| `getFlags`, `setFlagValue`, `getFlagValues` | same (first name wins) | differential `flags` |
| `getShortcuts`, `getShortcutDiagnostics` | same; diagnostics text byte-identical, `console.warn` → stderr | differential `shortcuts` |
| `invalidate`, `assertActive` | `invalidate`, `assert_active`; exact message, shared stale state | `stale_contexts_reject_actions`, `stale_context_reads_throw_the_typescript_error_text` |
| `onError`, `emitError` | same (ordered listeners, unsubscribe) | differential `errors` |
| `hasHandlers` | same (typed event-name match) | `session_shutdown_helper_reports_handler_presence` |
| `hasApprovalReviewers`, `hasToolGates` | same | compile-time |
| `getMessageRenderer`, `getMarkdownTransformers`, `getEntryRenderer` | same | `collects_renderers_and_transformers_in_load_order` |
| `resolveRegisteredCommands`, `getRegisteredCommands`, `getCommand`, `getPromptCommand` | same suffix/collision algorithm, internal source info | differential `commands` |
| `shutdown`, `getActiveTools` | same | runner tests |
| `createContext` | `create_context` (shared `ExtensionContextState`) | differential `context_live`, `context_members_read_live_runner_state` |
| `createCommandContext` | `create_command_context` (live method wrappers) | `command_context_passes_fork_options_through`, differential `context_live` |
| `emit` (generic session/compaction results) | `emit` → `EmitResult` | differential `session_events` |
| `emitRuntimeMailbox` | same (first `handled`, error reported then rethrown) | differential `mailbox` |
| `emitMessageEnd` | same (role check, last replacement wins) | differential `dispatch`, `errors` |
| `emitToolResult` | same patch merge; usage accepted but not returned | differential `dispatch`, `tool_result_usage_is_not_forwarded` |
| `emitToolCall` | same last-result/block short-circuit; input patch applied between handlers | differential `dispatch`, `mutations` |
| `emitToolGates`, `emitApprovalReviewers` | same, input patches applied | `gates_and_reviewers_patch_tool_input` |
| `emitUserBash` | same first-result-wins | differential `dispatch` |
| `emitContext` | same chaining; abort races handler | differential `dispatch`, `errors`, cancellation test |
| `emitBeforeProviderRequest` | same replacement chaining | differential `provider_request` |
| `emitBeforeProviderHeaders` | same chaining; headers returned as patch | differential `mutations`, `dispatch`, `errors` |
| `emitBeforeAgentStart` | same message collection + system-prompt chaining with live `getSystemPrompt()` | differential `dispatch` |
| `emitResourcesDiscover` | same ordered path collection | differential `dispatch` |
| `emitInput` | same transform/handled chaining | differential `input` |
| `applyApprovalReviewerResult`, `replaceToolInput` | same | `gates_and_reviewers_patch_tool_input` |

## Gaps

- None. `setSandboxProfile` is bound through
  `ExtensionCommandContextActions::set_sandbox_profile` (the session persists it
  and updates the bwrap status); `relocate`/`relocateAfterToolResult` are
  callbacks with the source default errors; `getDetachedJobLifecycle` is bound
  through `ExtensionRunner::bind_detached_job_lifecycle`.

## Discrepancies

- Handlers receive owned event payloads; contexts are shared live-state
  handles. Chained mutation is preserved
  by applying returned patches between handlers (`ToolCallEventResult.input`,
  `BeforeProviderHeadersEvent` → `ProviderHeaders`). The differential
  `mutations` scenario proves later handlers observe earlier mutations and the
  final input/headers reach the caller.
- `emitContext` passes clones and adopts returned messages; TypeScript passes
  the same array reference, so in-place `event.messages.push` without a return
  is not observable in Rust.
- `emitBeforeProviderRequest` adopts only returned payloads; TypeScript also
  observes in-place payload mutation.
- `emitInput` compares images by value, not reference.

## Representation

- Runner-owned context values live in `ExtensionContextState` behind a mutex
  shared by the runner and every context it creates. Context members are
  fallible methods that assert the runner is active and read live state, so
  rebinding, UI-mode changes, cwd updates, and callback-backed values are
  observed after `createContext()`, matching the TypeScript getters.
- `set_cwd` is a Rust-only runner setter used to express the live-cwd scenario;
  the TypeScript oracle assigns the private `runner.cwd` field at runtime.

## Live registration identity (2026-10-20)

`Extension` is a cloneable handle to one `Arc<Mutex<ExtensionState>>`.
`load_extension_from_factory` returns that handle, not a cloned registry record.
API, loader metadata enrichment, runner, and session resource clones therefore
observe the same state. Mutex guards must not span callbacks or awaits.

| Registration | TS loader evidence | Live Rust reader / proof |
| --- | --- | --- |
| tools | `registerTool` mutates `extension.tools`, calls `runtime.refreshTools()` | runner `get_all_registered_tools` / `get_tool_definition`; `runtime_registration_from_command_is_visible_to_runner` |
| commands | `registerCommand` mutates `extension.commands` | runner command resolution; runtime registration test executes late command |
| flags | `registerFlag` mutates flags; default only if absent | runner `get_flags`, runtime flag values; runtime registration test |
| shortcuts | `registerShortcut` mutates shortcuts | runner `get_shortcuts`; runtime registration test |
| message / entry renderers | register methods mutate corresponding maps | runner renderer lookups; runtime registration test |
| markdown transformer | replaces `extension.markdownTransformer` | runner `get_markdown_transformers`; runtime registration test |
| event handlers | `on` appends to the same event list | all runner dispatch paths read callbacks from shared state without holding a guard during callbacks |
| approval reviewers / tool gates | registration appends to corresponding arrays | runner presence checks and dispatch read shared lists; existing gate/reviewer behavior tests |
| providers / native providers / unregister | delegates directly to shared runtime | no extension snapshot involved; queued/immediate provider tests unchanged |
| event bus subscriptions | delegates to shared event bus/runtime subscription tracking | no extension snapshot involved; existing event-bus tests |

Normal callback arrays use live index iteration (including append during dispatch),
matching JS `for...of`. `emitRuntimeMailbox` intentionally gathers callbacks before
dispatch, matching TS `flatMap` at runner.ts:1104–1106.
Tool activation policy remains in `AgentSession::refresh_tool_registry`: registration
immediately refreshes definitions; newly seen permitted names become active;
re-registering an inactive existing name does not reactivate it. Source:
agent-session.ts:5096–5196. No extra synchronization or notification path added.
