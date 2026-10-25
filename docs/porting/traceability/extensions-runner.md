# extensions/runner.ts → `core::extensions::runner`

Source: `packages/coding-agent/src/core/extensions/runner.ts` (1539 lines, frozen `4f9e6f8a4`).
Whole-module audit closed 2026-10-25. All dispatch, query, relocation, command/default-action and context branches are mapped below. Current proof: [extension API closure](../ext-api-repin-proof.md); it supersedes historical pending labels.
Rust: `crates/pi-coding-agent-tools/src/core/extensions/runner.rs`.

Differential: `tests/extensions.rs::extensions_match_the_pinned_typescript_runner`
drives the real TypeScript runner through 14 scripted scenarios (`dispatch`,
`errors`, `shortcuts`, `commands`, `flags`, `input`, `input_images`, `project_trust`,
`providers`, `mailbox`, `session_events`, `mutations`, `context_live`,
`provider_request`).

## Q15 current oracle and input transform

Oracle imports actual ModelRegistry.inMemory/AuthStorage, not retired model-runtime-test-utils. Frozen runner.ts:1505 explicitly structuredClone(result.images); native now makes a fresh InputImages even when the handler returns its input unchanged. RED `q15b-extensions-current-3.log` observed continue versus frozen transform. Old-only header callback portions are deleted, not recreated as a facade; every remaining frozen dispatch/error/mutation assertion remains exact. No new runtime deviation. Context oracle reads current getScopedModels()/getThinkingLevel() methods (frozen:885–895), not retired properties. GREEN full scenario matrix: `q15b-runner-current-6.log`; scalar mutation fixture crosses JsValue::to_serde explicitly rather than serializing JsObject keys through the wrong serde map representation.

## Q13 recovery helper (4b checkpoint)

Frozen :497–506 delivers a queued tool relocation through `prepareWorkingDirectoryRelocation`; native now factors the same method and exposes it to afterTurn recovery. Callback is cloned outside the live-state guard, awaited first, then prepared is set only on success. Missing target still returns without a callback. Two actual-frozen/native preparation scenarios and a real provider-turn prepare→activate case are GREEN in the [4b matrix](agent-session-runtime.md#4b-recovery-checkpoint-2026-10-23); existing prepared target is retained without reacquiring its held permit. No whole-runner acceptance inferred.

## Readonly context boundary (2026-10-25)

`createContext` now supplies the existing readonly façade through `ExtensionContext::session_manager`, matching frozen runner's `sessionManagerReadView`. Native readers no longer acquire a core mutex. Goal state helpers accept the façade and keep only the two allowed setters, matching frozen `extensions/goal/src/goal-state.ts:20,110`. No production unrestricted writer remained after the typed getter migration. Test harness fixture writers retain their own explicit core authority; runtime rename uses the bound session action rather than a context write. Core history traversal accepts the branch/file inputs directly, without changing traversal logic.

RED: extension-facing direct snapshot/goal methods did not compile because the getter returned a mutable Arc (`/tmp/ext-api-readonly-red.log`). GREEN: new snapshot/live/goal/stale-context test, existing persisted goal/live-view test and real runtime replacement/rebind/name-read test, all focused (`/tmp/ext-api-readonly-{green,core,rebind}.log`). Workspace/all-target compilation passed (`/tmp/ext-api-readonly-all4.*`); its sole unused test import was subsequently removed. No broad test suite run. Parent integration `cad6ade44` was merged before the getter slice.

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
| `getMessageRenderer`, `getEntryRenderer` | same | `collects_renderers_in_load_order` |
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
| Retired `emitBeforeProviderHeaders` / before_provider_headers | Deleted event/type/registry/API/dispatcher and SDK transform consumer | Exclusive old-only callbacks/output/assertions retired; zero frozen-package occurrences; other dispatch assertions retained |
| `emitBeforeAgentStart` | same message collection + system-prompt chaining with live `getSystemPrompt()` | differential `dispatch` |
| `emitResourcesDiscover` | same ordered path collection | differential `dispatch` |
| `emitInput` (:1481–1520), explicit structuredClone at:1505 | Transform clones supplied InputImages before identity comparison | differential `input` + `input_images`: same-array return and fresh copy both transform; omitted images retain prior identity |
| `applyApprovalReviewerResult`, `replaceToolInput` | same | `gates_and_reviewers_patch_tool_input` |

## Gaps

- None. `setSandboxProfile` uses constructor-provided settings/session state directly and updates the current UI's bwrap status; `relocate`/`relocateAfterToolResult` are
  callbacks with the source default errors; `getDetachedJobLifecycle` is bound
  through `ExtensionRunner::bind_detached_job_lifecycle`.

## Discrepancies

- Handlers receive owned event payloads; contexts are shared live-state
  handles. Chained mutation is preserved
  by applying returned ToolCallEventResult.input patches between handlers. The differential
  `mutations` scenario proves later handlers observe earlier mutations and the final input reaches the caller. Old-only header callbacks are deleted.
- `emitContext` passes clones and adopts returned messages; TypeScript passes
  the same array reference, so in-place `event.messages.push` without a return
  is not observable in Rust.
- `emitBeforeProviderRequest` adopts only returned payloads; TypeScript also
  observes in-place payload mutation.
- Both follow the [approved return-only deviation](../../specs/compatibility.md#out-of-scope)
  recorded in `d6b4179c`. The
  `provider_request` differential therefore differs on purpose: the oracle
  (`oracle/extensions-runner-oracle.ts:725`) mutates `payload.b` without
  returning, while the native fixture returns `Some(payload)`. Its final
  `null` replacement means the scenario proves chaining and null replacement,
  not no-return mutation.
- `emitInput` uses InputImages identity. Every supplied transform image array is detached before the final identity check, matching frozen structuredClone; omitted images retain the previous array. The current `input_images` differential proves both cases.

## Sandbox setter audit (2026-10-25)

Frozen runner.ts:949–973 owns the sandbox setter independently of command-action binding. Native had incorrectly put the algorithm on AgentSession behind a native-only command-action callback. Moved the existing persistence/status algorithm into the runner/context authority; AgentSession delegates there. Removed the native-only callback field and its mechanical host/fixture initializers. Uses the current UI theme, source scope errors and unsupported-platform status; resets of command bindings do not remove settings capability. Runner instance identity and background-event variants are untouched.

Actual frozen/native constructor-settings global/session/reset matrix RED `/tmp/ext-api2-sandbox-red.log`: all native calls incorrectly reported missing settings. GREEN `/tmp/ext-api2-sandbox-green2.log`. [Contract](../../specs/extension-sandbox-profile.md). The approval-controls command matrix retains every original argument/registration assertion but now compares actual frozen runner settings, errors and UI status/notifications instead of a mock setSandboxProfile logger that bypassed all persistence/validation. This is fixture fallout from removing the native-only callback seam; no frozen command case is dropped.

## Frozen context/error audit (2026-10-25)

Actual frozen/native boundary tests found two mismatches. `createContext` captures the current `getModel` callback (frozen :822), unlike its other live callback-backed properties; native now captures that callback while still reading its current model value and asserting stale state. The earlier native-only pre-bind model assertions contradicted frozen and are replaced with exact frozen before-bind/after-bind/rebind behavior; mutable values behind a captured callback remain tested. This is literal frozen behavior, not an approved semantic fix.

Runner diagnostics, mailbox error reports, project-trust reports and factory-load diagnostics now use Error.message/String(rejection) through one HandlerError helper, preserving original rejections for propagation. Plain strings, objects, numbers, null and native UTF-16 strings previously emitted empty diagnostics. RED `/tmp/ext-api2-runner-boundary-red2.log`; GREEN in current `extension_session_read_view` boundary tests. Exotic String-coercion failure routes to the existing shared uncaught authority (pathological-input gap); no new carrier or catch/panic mechanism added.

## Representation

- Runner-owned context values live in `ExtensionContextState` behind a mutex
  shared by the runner and every context it creates. Context members are
  fallible methods that assert the runner is active and read live state (except the frozen captured model callback), so
  UI-mode changes, cwd updates, and other callback-backed values are
  observed after `createContext()`, matching the TypeScript getters.
- `set_cwd` is a Rust-only runner setter used to express the live-cwd scenario;
  the TypeScript oracle assigns the private `runner.cwd` field at runtime.

## Live registration identity (2026-10-20)

Runner stores shared extension handles, not load-time registry copies. See
[registration audit](extensions-loader.md#live-registration-identity-2026-10-20).
Queries read current tools/commands/flags/shortcuts/renderers; dispatch retrieves
one callback at a time and releases the registration guard before user code.
Live array iteration includes handlers appended during the same emission;
`emitRuntimeMailbox` retains its explicit gather-before-dispatch semantics.

Proof: `runtime_registration_from_command_is_visible_to_runner` covers late tools,
commands, flags, shortcuts, renderers, handlers, gates, and reviewers;
`handlers_appended_during_dispatch_run_in_the_same_event` covers live appends.
[Dynamic-tools proof](extension-example-dynamic-tools.md) covers the actual pinned
example and real AgentSession refresh/execution/deactivation timing.

## Q18 discovered resource contract (2026-10-22)

ResourcesDiscoverResult path arrays and DiscoveredPath path/extension_path use
JsString. The producer passes original callback values and native extension
identity to AgentSession; source labels/baseDir are composed with native path
operations. Keybinding/shortcut runner methods are untouched by this slice.
Current runtime proof is GREEN in the runner differential and native contract cases; see [closure proof](../ext-api-repin-proof.md).
