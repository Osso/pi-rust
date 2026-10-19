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
