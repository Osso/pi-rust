# interactive-mode (`coding-agent/src/modes/interactive/interactive-mode.ts`)

Ported to `crates/pi-coding-agent/src/modes/interactive/interactive_mode.rs`.

## Q15 login options/command checkpoint (2026-10-23)

[Contract](../../specs/login-options.md). Native login_options.rs extracts frozen6706–6751; no source login-options.ts exists. Exact OAuth-ID command replaces old name/case-insensitive/API-key/fuzzy routing. Old-only completion grouping/search/description and its sole interactive autocomplete consumer are deleted; no pi-tui consumers exist. Method/provider selectors now expose only the fixed frozen choices and return-to-method cancel transition.

`ui_tests::login_command_requires_exact_oauth_id_and_preserves_unknown_text` invokes the actual frozen method with the same OAuth provider dependency and compares native capture UI; q15-login-command-exact-id-red.log reproduces no native status versus frozen unknown-ID status. Native UTF-16 unknown IDs remain unchanged. `login_options_match_frozen_assembly_filtering_order_and_stored_credentials` exercises all/each auth-type filtering, OAuth/API-key duplication and truthy/false stored credentials against the actual source methods. GREEN/checkpoint gates are recorded separately in the lane proof ledger; no whole-mode or auth-callback acceptance is implied. Persisted unknown/missing credential.type remains an explicit error under the [approved gap](../../../AGENTS.md),2026-10-23. `logout_unknown_persisted_types_fail_explicitly_under_approved_gap` records source raw-value/undefined propagation and exact native errors for missing/string/number/null types; this gap row is classified, not a claim of parity.

## Q15 OAuth callback checkpoint (2026-10-23)

| Frozen capability | Native change | Proof |
|---|---|---|
| showOAuthLoginSelect cancel/empty ID/ordinary ID | optional queued response, no empty-string truthiness filter | Three actual-source/full original ANSI frame cases GREEN |
| showLoginDialog onAuth callback-server prompt | mount exact redirect input during onAuth, not during a read | Callback-server false/true original ANSI/OSC frames GREEN |
| one manualCodePromise per login | shared first settlement; exact raw UTF-16, repeated reads, empty stays pending, cancel rejects | Four actual-source process-isolated cases GREEN; owned xdg-open fake, no live browser/provider |
| progress/device/prompt callbacks | existing native paths, no production change | Three actual-source queued-UI scenarios GREEN10672618: full raw ANSI/OSC/UTF-16 frames, absent/empty/raw placeholders, raw/empty input and cancel; q15-oauth-prompt-device-progress.log |
| authentication completion/provider count/catalog consumers | existing native paths | separate owned leaves pending |

RED `q15-oauth-select-callback-red.log` records cancel→Login-cancelled versus frozen undefined. RED `q15-oauth-manual-callback-red-3.log` records the missing early input. GREEN `q15-oauth-checkpoint-tests.log` covers both repaired boundaries. Oracle uses the actual frozen TUI focus authority (requestRender only disabled); no frame sequences are stripped or replaced. Native queued dispatch preserves source dialog restoration and fixture rejection. Logs and exact revision scopes are in the Q15 proof ledger; whole-mode/final clippy/readability acceptance remains open.

## Reload streaming guard checkpoint (2026-10-23)

Frozen :7127–7130 refuses reload while streaming. Native used to reload the
session and emit “Reloaded”. `reload_while_streaming_matches_frozen_warning`
blocks a real faux provider response, invokes `/reload`, then releases and
completes that response; its warning matches the actual frozen handler exactly.
RED/GREEN: `reload-busy-{red,green}.log` in the Q13 proof directory. The rest of
reload (compaction guard, runtime lifecycle wrapper, loading box, full settings/
resources/shortcuts/theme refresh and chat restoration) remains rank-4 work.

## Viewed-session target / cycling checkpoint (2026-10-23)

Frozen :422–450 and :651–659 now have a per-mode resolver and explicit
`ResolvedMutationTarget::{Core, Interactive}` shape classification. Missing and
core-only children report the frozen errors with raw UTF-16 IDs; there are no
optional capability defaults. Main view uses the current live AgentSession.
`ViewedSessionMutationTarget::cycle_model` is required and the existing
AgentSession wrapper forwards to the session's cycle authority. Native errors
and raw HandlerError messages survive the CallbackError conversion.

`ui_tests::viewed_child_cycle_routes_live_targets_and_preserves_frozen_errors`
compares the actual frozen resolver/cycle methods against the capture-terminal
mode: missing/core-only child, interactive child with zero/one/two scoped models,
and main view. RED `viewed-target-red.log`: missing child emitted “Only one model
available”; GREEN `viewed-target-green.log`: six cases pass, main remains unchanged
when the child cycles. Logs live in `/tmp/pi-rust-repin-lossless-interactive/`.

Followup: `get_model_candidates`, `/model` and selector construction/callbacks
resolve the viewed target; the callback resolves again rather than retaining the
mounted target. Target errors propagate before the registry-only catch through
CallbackError. Three frozen/native model-command cases (exact, filtered Enter,
selector arrow+Enter) mutate child while main remains unchanged; RED
`viewed-model-red.log`, GREEN `viewed-model-green.log`.

Main mirrors frozen main.ts:1358–1373: construct once, post-bind once, then branch
benchmark init versus normal run inside LocalSet. No constructor option or
second target registry was added. `update_available_provider_count` propagates
the same carrier; login `complete_provider_authentication` → `run_provider_login`
preserves it to frozen catches :6976–6980/:7113–7117; logout catches its update at
:6838–6849. Existing StorageError::into_callback retains its original rejection.
Scoped-model callback raises at its native callback boundary only.

Footer followup: shared `build_selected_agent_footer_override` now uses persisted
model > published agent model > transcript model, with the same thinking-level
precedence and context-window estimate. Read failures are no longer suppressed
by `.ok()`: builder/render/open/slot/reload propagate CallbackError to the real
bridge/key/event boundaries. Model selection and cycling refresh both footer
consumers inside their frozen mutation catches.

`viewed_child_footer_prefers_persisted_then_published_then_transcript_model`
compares the actual frozen builder to real native manager/control-DB state for
all three precedence branches. RED `viewed-footer-red.log`: published model was
ignored. GREEN `viewed-footer-green.log`: three viewed-child tests pass, including
persisted child model mutation updating the footer without changing main.
Rank-3 proof followup (`viewed-proof-green.log`): six candidate cases establish
main/missing/core/interactive resolution, scoped-child candidate use, and actual
TypeError + `code` + raw surrogate rejection preservation before the registry
catch. The selector rebind differential mounts on child A, changes the resolver
to child B, then sends arrow/Enter: only B changes and main/A remain unchanged.
Five targeted viewed-child tests pass (19 source/native scenarios total).
The retained 11-case main-model regression also passes
`viewed-main-model-regression-phase.log`. Its old fixed 1ms delay raced deferred
publication (initial mounted rows versus source's loaded rows); the fixture now
waits boundedly for observable loaded/no-match rows before keys/frame comparison.
No production timing or selector behavior was changed for this fixture issue.
This closes this bounded ranked finding, not whole-mode parity. Main owns final
clippy/readability/independent gates; whole-mode row remains TODO.

## Signal action lifetime (2026-10-23)

Frozen :5241–5256 starts independent async actions with void restartProcess/shutdown; removing the listener does not cancel a started promise. Native signal-listener tasks now spawn owner-local action tasks instead of awaiting within the listener that stop() aborts. This fixes the executor regression: beforeProcessRestart stopped UI, aborting the SIGHUP listener while it awaited the runtime response, dropping the restart failure and leaving an idle stopped process.

Actual pre-merge baseline is ffb6782c^1 (34be64c0), not 465b2e54 (a descendant of ffb6782c). Existing interactive_crash SIGHUP failure test passed at34be64c0 and timed out atffb6782c; `crash-actual-pre-executor.log` and `crash-runtime-ffb6782c.log`. The four worker stack overflows occur on both revisions; subsequent default-stack correction is recorded below.

GREEN `sighup-action-green-2.log`: native PTY exits1, restores bracketed paste before Error report, matching actual frozen registerSignalHandlers→runtime restart→beforeRestart→stop→restarter rejection in `oracle/interactive-sighup-failure-oracle.mts`. Frozen stop :7844–7850 removes its :5279–5281 uncaught listener before rejection, so the source emits a default Error report, **not** the pi crash banner. The old positive-banner assertion was replaced by this actual source-process comparison; exit/order assertions are retained. `main_interactive`9/9 passed before this change; post-fix focused regressions remain separately ledgered.

## Default-stack worker startup (2026-10-23)

Existing interactive_crash worker_panic_child process tests overflowed before terminal initialization on34be64c0 and later code. A temporary injected16MiB child stack made the unchanged worker scenario pass; the final fix does not raise stack limits. Temporary diagnostics measured main's inline future at76,192 bytes and ELF poll prologues reserving153,496 bytes in main,469,304 in run_main_inner and153,624 in the worker wrapper (artifacts worker-main-future-size.log/worker-poll-prologues.log). The runner now boxes run_main_inner at its creation boundary so outer async/runtime wrappers retain only the handle rather than repeatedly carrying its state inline. Temporary size diagnostics and stack override were removed.

GREEN `crash-default-stack-green.log`: interactive_crash12/12 executable parent tests pass, including all four worker cases, SIGHUP failure and resource_display_home; three ignored entries are child helpers invoked by those parents. Source main changes only this allocation boundary, not lifecycle behavior or error handling.

## Header registration checkpoint (2026-10-23)

[Contract](../../specs/interactive-header.md). Frozen :285–287 checks non-null object, presence of setExpanded, and typeof setExpanded === function. Native header-only registration preserves `{component, set_expanded: Option<callback>}`; no trait capability/default method is added. Factory and expansion failures retain CallbackError. UI-owner set_header executes synchronously; workers use the existing UiRequest plus a response. Startup :1034–1088 now renders the exact compact/expanded keyHint text rather than old hardcoded, always-expanded text. Header registration/restoration :2724–2762 preserves built-in identity and surrounding spacers, disposes outgoing custom components and applies the actual optional callback. Reload :7178–7181 invokes the same callback for the active header.

`header_startup_and_registration_frames_match_frozen_expandability` uses the actual frozen startup AST block and ExpandableText/isExpandable declarations plus real frozen setExtensionHeader, keybindings and Text. Native real header/container/factory output matches full UTF-16/ANSI frames for compact, expanded with custom keys, quiet and verbose states, each with expandable/plain replacement and built-in restore (16 frames). RED `header-registration-red.log`: old startup content and missing expansion; GREEN `header-registration-green-2.log`. Initial custom fixture returned raw unpadded lines unlike its source Text; native fixture now uses the same real Text behavior, with unchanged exact assertions. Header error/worker/reload branch proofs remain pending. The 24 PTY startup waits/assertions in main_interactive, interactive_crash and resource_display_home now use the source-proven compact onboarding phrase `to show full startup help and loaded resources.` instead of the removed hardcoded `Ctrl+C twice to exit`; exact source-frame assertions remain unchanged.

## Reload body checkpoint (2026-10-23)

Frozen :7136–7219 now mounts the actual reload box, yields before work, calls runtime.reload under its single permit, restores chat before session_start (once), reloads HTTP/keybindings/themes, applies editor/cursor/shrink settings and autocomplete, displays resources/model errors and the full success status, then restores the editor on success or the previous editor on failure. Extension command reload uses the same handler, not a parallel session.reload path.

`ui_tests::reload_body_matches_frozen_loading_settings_status_and_raw_failure` compares the actual source method and real source SettingsManager to the native capture-terminal host with a suspended ResourceLoader.reload, then fulfills or rejects with a raw lone surrogate. Observed output: pending loading frame, final editor settings, cursor/shrink settings, exact UTF-16 status/error lines and restored editor mount. RED `reload-body-red.log`: regular editor remained visible, settings remained old, status was only Reloaded; GREEN `reload-body-green-2.log`: both cases pass. Oracle fixture initially bypassed SettingsManager's setter clamp; frozen settings-manager.ts:1397–1399 clamps padding 7 to3. Fixture now uses that real setter; no assertion weakening or production clamp change.

Compaction guard :7131–7134 is now source/native proven by `reload_while_compacting_matches_frozen_warning_without_reloading_resources`: suspend a real compaction in its source-hint extension hook (isCompacting true, isStreaming false), request reload, then release compaction. RED `reload-compacting-red.log` reloaded successfully; GREEN `reload-compacting-green.log` displays the frozen warning without running reload.

Implicit trust :6012–6036/:7201–7209 now persists and clears the one-shot cwd flag at frozen boundaries; path/resource/constructor failures propagate to the reload catch, while trust-store read/write failures produce the frozen warning. `reload_implicit_trust_matches_frozen_persistence_and_status_branches` compares actual frozen reload + implicit-trust helper + source trust store/settings with the native real reload. Six cases: initial save, existing true/false, mismatched cwd, untrusted settings, absent resources. RED `reload-trust-red.log`: source persisted true, native remained null. The original GREEN `reload-trust-green-2.log` was invalid for the untrusted display branch: the oracle mock incorrectly invoked renderProjectTrustWarningIfNeeded to match native. Corrected oracle now invokes actual frozen rebuildChatFromMessages→renderSessionEntries→renderSessionItems (:5078–5081), which has no initial-load footer/history/trust/compaction notices. `reload-rebuild-literal-red.log` reproduces native's extra trust warning with unchanged assertions. Native rebuild now calls render_session_entries only; GREEN `reload-rebuild-literal-green.log` proves all six persisted-state/full-status cases against that literal source authority. Held-lock catch characterization remains pending.

Extension shortcut installation :2215–2273/:7195–7196 is now live after initial extension bind and reload; default editor key handling dispatches matching shortcuts asynchronously and catches raw handler errors at the frozen display boundary. `extension_shortcuts_bind_and_reload_match_frozen_input_context_and_errors` compares actual source setupExtensionShortcuts + real source CustomEditor input with actual native extension registration/editor input: initial bind and post-reload, successful raw UTF-16 editor mutation and failed raw handler message (four cases), plus cwd/hasUI/idle/trust context values. RED `extension-shortcuts-red.log`: no handler/context/editor mutation; GREEN `extension-shortcuts-green.log`: all cases pass. Source captures the runner's context while its active session getters remain live; full compact/restart/abort context-action auditing is still part of the whole-mode matrix.

Still pending in rank4a: active-header expansion; individual pre-start transcript ordering and catch-side-effect branch proofs. Existing streaming guard remains proven. This is not full /reload or whole-mode parity.

## Runtime owner hooks (2026-10-23)

Frozen :666–672 constructor hooks now use the one runtime lifecycle authority. UI session reads use runtime.session; alternate adopt_session state/call paths are deleted. Subscriptions retain their outgoing AgentSession alongside the ID, so replacement unsubscribes from the correct owner. Native rebind follows :2142–2203 render-before-bind, short settings application, live extension binding and footer/title/cwd refresh. UI requests schedule lifecycle work with spawn_local rather than awaiting inside the drain. Worker prepare→activate RED/GREEN and interleaved rebind/order differential: [runtime proof](agent-session-runtime.md#ui-owner-dispatch-checkpoint-2026-10-23). Reset/restart branch side effects and bindCurrentSessionExtensions extras still require individual proof; not whole-mode acceptance.

## Deferred session-selector default-directory failure (2026-10-23)

Frozen interactive-mode.ts:6580/6612 evaluates usesDefaultSessionDir only inside the all/archived loaders; session-selector.ts:1005–1017 catches and displays Error.message. Native previously evaluated eagerly while mounting current scope and raised uncaught. Both native loaders now evaluate the live manager lazily, preserving the error until the selector's message boundary. The in-crate session_selector_config_failure test compares a frozen real-selector oracle (zero initial lookups, one all-scope lookup, caught status) and a native capture-terminal subprocess (current scope mounts, Tab displays the native URL error without a process crash). RED: selector-config-catch-red-2.log; GREEN: selector-config-catch-green-differential-2.log.

## AbortSignal consumer audit (2026-10-22)

[Contract](../../specs/interactive-abort.md) · [Shared carrier](shared-abort.md). Parent's workspace-wide signature swap is the starting boundary, not reason parity evidence.

| Frozen consumer | Current state | Proof |
|---|---|---|
| reportInstallTelemetry passes AbortSignal.timeout(5000) to Fetch | shared timeout signal; correct branded TimeoutError/code23 and retained lifetime | actual unmodified source method with mock global Fetch versus real native method with mock Fetch, both pending and immediately completed requests; test-interactive-timeout-reason-red-1.log → green-1.log,1/1 test covering2 real process branches |
| CancellableLoader / BorderedLoader / LoginDialog / editor cancellation | mechanical default-reason producers appear source-aligned; direct boundary proof pending | shared carrier proof does not replace the pending per-consumer audit |
| autocomplete fd completion/abort | mechanical signal preserves reason; late-abort completion ordering still being investigated | not accepted or fixed without reproduced RED |
| login/model-registry15s refresh block | old-only model-runtime path; Q15-owned | not touched by this checkpoint |

The RED observed AbortError/code20 for a pending request and no abort at all after an immediately completed request. Native manual controller/timer cleanup was replaced by the one shared timeout authority, not a new error constructor or fallback. Tests observe supplied Fetch signal metadata/lifetime; all requests are mocked and no network occurs. JS stacks are omitted. Other telemetry conformance helpers/docs remain approved gaps, and no telemetry enablement/statistics behavior or whole-method scheduling parity is claimed. Parent owns final gates.

## Session info and old-only cache removal (2026-10-22)

[Contract](../../specs/session-info.md). The actual frozen private handleSessionCommand is invoked without source edits on a minimal readonly receiver. Its real Text/Container frames are compared against the real native mode handler at widths25/120, in-memory/persisted sessions, empty/populated usage and raw UTF-16 names: eight states. RED `test-session-info-red-1.log`; GREEN1/1 `test-session-info-green-1.log`. Native now emits the exact Session Info/messages/token layout, individual input/cache fields, nullish In-memory file fallback and four-decimal positive cost without $. The old outer-dim show_status route, per-model cost and cache-waste display were removed. The one shared en-US number formatter retains fractions; [numeric proof](js-compat.md).

Deleted old-only cache_stats module/export/tests, notice insertion at replay and MessageEnd, notice/price helper methods, and cache-notice/Mermaid settings fields/APIs/rows/callbacks. Source has none. The old positive notice test is replaced by actual MessageStart→MessageEnd behavior with preserved unknown legacy config: RED emitted Cache miss after10m idle, GREEN emits current assistant content without any notice (`test-cache-notice-removal-{red,green}-1.log`). Settings-query RED→GREEN removes both obsolete options; native retained settings regressions6/6 and output-pad sibling1/1 pass. [Removal classification](cache-stats.md).

The command is /session; no /stats alias was added. These bounded source-handler/event proofs do not establish whole-mode parity; settings-selector and other current coverage gaps remain open. Parent owns final readability/clippy/independent gates.

## Frozen single-renderer reconciliation (2026-10-22)

Frozen `interactive-mode.ts:674` constructs one `TUI`; its stop paths (:5150–5176, :5317) call `ui.stop()` without alternate-screen output modes. Removed unreachable native `switch_tui_mode`, fullscreen teardown/transcript rerender, shared fullscreen flag and fullscreen-only short copy confirmation. Removed old-only settings rows/config/callbacks for TUI mode, fullscreen exit output and scrollbar; frozen `settings-selector.ts` contains none. Existing regular teardown/editor/settings behavior remains.

Deleted four old-only failing UI tests (fullscreen message placement, transcript-on-fullscreen-exit, fullscreen resume-hint preservation, and TUI-mode setting switch). Their RED logs are `lib-*-95fcd282.log`; none has a frozen-valid fullscreen remainder. Retained unrelated status/persist tests. The base fixed-cell extension factory test now explicitly uses regular renderer; it still checks real custom editor input. Removed CLI `--tui-mode`, typed interactive options, help entry and known settings accessors/enums. The removed CLI name follows frozen unknown-flag handling rather than gaining a compatibility alias. Persistent legacy keys remain arbitrary settings data; [settings-manager proof](settings-manager.md#legacy-renderer-key-persistence-2026-10-22) compares frozen load/flush/unrelated-save/reload exactly.

## Exact followup slice parity (2026-10-22)

Former String-boundary U+FFFD approval is superseded by the explicit exact-parity decision. Frozen :407–416 slices the body at 50 UTF-16 units and returns the raw split surrogate. Native production already retains that unit; the old replacement-character expectation was RED (`lib-followup_slices_raw_units_before_display_encoding-95fcd282.log`). The test now compares native JsValue output to the pinned function executed by `interactive-followup-oracle.ts`; the oracle extracts/executes the actual private helper and its constant, not a rewritten algorithm. Cases cover split astral body, goal sender and raw unpaired units. No production scalar projection or fallback introduced.

## Native POSIX parse directory (2026-10-22)

Frozen interactive-mode.ts:1561 uses `path.posix.parse`, whose bare-file directory is empty, not dirname's `.`. Native `parse_posix_path` now extracts the directory at the last non-trailing separator without normalization and preserves root/repeated separators. Existing `parsed_posix_paths_match_node_shape` reproduced bare-file `.` versus empty RED; added local Node-confirmed root/trailing/repeated/empty cases. No user-content encoding or filesystem access occurs in this helper.

## Native extension bridge checkpoint

`UiRequest` and `InteractiveUiContext` dialog arguments/results, widget/status keys/content, title/editor text, custom errors and working/hidden labels retain `JsString` through queues and mounted dispatch. SelectAgentView IDs are native. No encoding adapter is added. In-module `lossless_ui_bridge_tests` covers raw status/widget/title/selection request-response units; compilation was RED in `/tmp/q18-ui-context-red.{jsonl,log}` and execution remains blocked by incomplete whole-mode/coding migration. Theme names and remaining scalar host callers are separate pending work; this checkpoint is not whole-mode acceptance.

## Lossless checkpoint (2026-10-21)

Compaction and branch-summary custom instructions retain `JsString` through
session calls; scalar editor input is promoted at ingress. Session-selector
rename input is promoted before the raw control-DB call. Cached footer names
remain raw until rendering ([footer boundary](interactive-footer.md)).
The historical completion claims below do not establish this migration's proof:
Rust tests remain blocked by out-of-lane library diagnostics. Existing raw
renderer/RPC/parser tests are retained, not duplicated. Scalar Text/Markdown,
editor/select/input and TUI-internal truncation remain an unapproved surrogate
display gap; no raw lookup or stored content may use that display projection.

Status: **whole common-pin module remains TODO**; the historical implementation inventory below is not re-pin acceptance. The mode core (construction, TUI layout,
startup resources, session event rendering, editor submission, key handlers,
slash commands, extension UI dialogs, selector components, login/logout,
suspend, clipboard paste, share viewer, startup changelog/telemetry, prompt
activity, render-region layout callbacks, subscription warning and easter
eggs) plus agent slots, the working loader, child footer overrides and
notify-based child transcript reloads are implemented and behavior-tested.

## Exported symbols

| Source symbol | Rust item | Tests |
| --- | --- | --- |
| `InteractiveMode` | `InteractiveMode` | `ui_tests::renders_startup_screen_and_prompt_submission`, `ui_tests::bash_mode_executes_and_renders_command_output` |
| `InteractiveModeOptions` | `InteractiveModeOptions` | construction in `ui_tests::build_mode` |
| `createInteractiveTui` | `create_interactive_tui` | exercised by `build_mode` (regular); fullscreen renderer construction covered by `pi-tui` renderer tests |
| `createInteractiveTuiReference` | `InteractiveTuiHandle` (stable renderer handle) | construction/renderer swap used by components; no direct test |
| `bindInteractiveModeSessionMutationTargetResolver` | `bind_interactive_mode_session_mutation_target_resolver` | viewed-child cycle/model differential; main post-binds like frozen main.ts:1371 |
| `stop(fullscreenExitOutput = getFullscreenExitOutput())` / `stopInteractiveTui` | `InteractiveMode::stop` / `stop_with_output` / `stop_interactive_tui` | `ui_tests::stop_renders_the_transcript_when_leaving_fullscreen`, `ui_tests::stop_with_resume_hint_keeps_the_fullscreen_renderer`, `ui_tests::stop_clears_terminal_progress_and_disposes_footer_components`, `PI_STARTUP_BENCHMARK` path in `main_runner` |
| `processControlMessage` | `process_control_message` | control-message branch of `run`; `IncomingControlMessage` completion/failure |
| `InteractiveTuiHandle` dispatch | `InteractiveTuiHandle::renderer()` clones the active renderer | pty interactive test (input callbacks request renders) |
| `formatResumeCommand` | `format_resume_command` | unit tests for quoting; `main_interactive::quit_prints_the_resume_command` covers the TTY branch and the `shutdown` output line |
| `custom()` async settlement and rejection-vs-close state (`interactive-mode.ts:3458–3534`) | `show_extension_custom` invokes the factory synchronously, awaits its UI-local future on the LocalSet, mounts only after fulfillment; outer errors remain synchronous throws | `ui_tests::extension_ui_async_factory_settlement_orders_mount_restore_and_early_done`: eight real-host fulfill/reject/early-Done × overlay cases; pending editor visibility/focus, exact restored text, handle count, Enter completion |
| Overlay unfocus explicit target (`tui.ts:257–276,828–858`) | `InteractiveOverlayHandle::unfocus_to` forwards native `ComponentRef` or null; omitted `unfocus` unchanged | `ui_tests::extension_ui_unfocus_distinguishes_omitted_explicit_target_and_null` |
| `custom()` non-overlay saved editor text (`interactive-mode.ts:3471–3479`) | `show_extension_custom`: capture before factory, restore only on non-overlay Done | `ui_tests::extension_ui_custom_restores_text_saved_before_construction_only_without_overlay`: factory-time mutation, open-component `set_editor_text`, actual Enter completion; overlay retains changed text. Synchronous failure retains `extension_ui_factory_error_rejects_custom_without_changing_focus_or_overlay` proof. |
| `ExtensionUIContext` implementation | `InteractiveUiContext` | `ui_tests::extension_select_dialog_resolves_and_restores_editor` (select); confirm/input/editor share the same broker |
| `showSettingsSelector` | `show_settings_selector` | `ui_tests::settings_selector_updates_http_timeout_and_writes_settings_file`, `ui_tests::settings_selector_switches_tui_mode_and_reports_status`, `ui_tests::settings_selector_updates_session_auto_compact_and_thinking_level` |
| `showModelsSelector` / `persistEnabledModelsForSelector` | `show_models_selector` / `persist_enabled_models_for_selector` | `ui_tests::scoped_models_selector_updates_session_scope_and_persists_selection` |
| `showTrustSelector` | `show_trust_selector` | `ui_tests::trust_selector_saves_decision_and_reports_status` |
| `showSandboxSelector` / `updateBwrapStatusForSandboxProfile` | `show_sandbox_selector` / `update_bwrap_status_for_sandbox_profile` | `ui_tests::sandbox_selector_saves_profile_and_updates_bwrap_status` |
| `showApprovalSelector` | `show_approval_selector` | `ui_tests::approval_selector_saves_preset_and_reports_status` |
| `showUserMessageSelector` | `show_user_message_selector` / `fork_from_entry` | `ui_tests::fork_selector_forks_from_the_selected_user_message` |
| `showTreeSelector` / `handleTreeSelectionWithoutNavigation` / `startCompactionSummaryEdit` / `materializeCompactionSummaryForEdit` | `show_tree_selector` / `handle_tree_selection` / `start_compaction_summary_edit` / `materialize_compaction_summary_for_edit` | `ui_tests::tree_selector_navigates_to_the_selected_entry` |
| `showSessionSelector` / `handleResumeSession` / `continueInterruptedResumedSession` | `show_session_selector` / `handle_resume_session` / `continue_interrupted_resumed_session` | `ui_tests::resume_selector_switches_to_the_selected_session` |
| `handleLoginCommand` / `showLoginAuthTypeSelector` / `showLoginProviderSelector` / `startProviderLogin` / `showLoginDialog` / `showApiKeyLoginDialog` / `completeProviderAuthentication` | `handle_login_command` / `show_login_auth_type_selector` / `show_login_provider_selector` / `start_provider_login` / `show_login_dialog` / `show_api_key_login_dialog` / `complete_provider_authentication` | `ui_tests::login_selector_opens_the_ambient_auth_dialog`, `login_options::tests::api_key_login_availability_follows_the_auth_method` |
| `maybeWarnAboutAnthropicSubscriptionAuth` | `maybe_warn_about_anthropic_subscription_auth` | `ui_tests::anthropic_subscription_auth_warning_is_shown_once`, `ui_tests::anthropic_subscription_auth_warning_respects_the_settings_opt_out` |
| `renderPreparedUserMessage` / `consumePreparedUserMessage` / `removeProvisionalUserMessages` / `submitMainLoopInput` (`onUserMessagePrepared`) | `render_prepared_user_message` / `consume_prepared_user_message` / `remove_provisional_user_messages` / `prepared_user_message_callback` (thread-local mode bridge because `PromptOptions` callbacks are `Send + Sync`) | `ui_tests::prepared_user_message_renders_at_submit_and_is_reused` |
| — provisional matching detail | TS keys `provisionalUserMessages` by object identity; Rust matches the `AgentMessage` value (`PartialEq`) because the core event carries a clone. Same timestamp/content pairs are the only ambiguity. | `ui_tests::prepared_user_message_renders_at_submit_and_is_reused` |
| `handleFatalRuntimeError` (:2494) | `handle_fatal_runtime_error` (show error, `stopThemeWatcher`, `stop("transcript")`, exit 1); called from `/clear`, extension `newSession`/`fork`, `/resume`, and `/import` non-file-not-found failures | `main_interactive::import_failure_exits_through_the_fatal_runtime_handler` (process exit 1); other call sites share the helper |
| `handleShareCommand` | `handle_share_command` / `handle_share_command_with_program` | `ui_tests::share_command_creates_a_gist_and_reports_the_viewer_url`, `main_interactive::share_command_reports_a_missing_gh_login` |
| `getChangelogForDisplay` / `reportInstallTelemetry` / `showStartupNoticesIfNeeded` | `get_changelog_for_display` / `changelog_display_for` / `report_install_telemetry` / `show_startup_notices_if_needed` | `ui_tests::changelog_display_returns_only_entries_newer_than_the_seen_version`, `ui_tests::startup_notices_render_new_changelog_entries`, `ui_tests::install_telemetry_reports_the_version_through_the_configured_fetch` |
| `setPromptActivity` / `syncWorkingEditorState` | `set_prompt_activity` / `sync_working_editor_state` | `ui_tests::prompt_activity_animates_the_prompt_until_the_source_finishes` |
| `handleEditorLayout` / render-region `place` callbacks | `rebuild_root_compositor` layout callbacks | `ui_tests::editor_layout_callback_positions_the_working_editor` |
| `applyFullscreenScrollbarSetting` / `scrollbarStyle` | `build_fullscreen_layout` `scrollbar_style` | `ui_tests::fullscreen_scroll_view_uses_the_scrollbar_thumb_background` |
| `handleArminSaysHi` / `handleDementedDelves` / `handleDaxnuts` / `checkDaxnutsEasterEgg` | `handle_armin_says_hi` / `handle_demented_delves` / `handle_daxnuts` / `check_daxnuts_easter_egg` | `ui_tests::daxnuts_easter_egg_triggers_for_opencode_kimi_models` |
| `showOAuthSelector` (logout) | `show_oauth_selector(true)` | `ui_tests::logout_selector_removes_the_stored_api_key` |
| `setEditorComponent` / `getEditorComponent` / `addAutocompleteProvider` | `set_custom_editor_component` / `get_editor_component` (shared factory cell) / `add_autocomplete_provider` | `ui_tests::extension_context_exposes_autocomplete_and_editor_factories`, `ui_tests::extension_select_dialog_resolves_and_restores_editor`, `tests/differential_interactive_components.rs` |
| `AGENT_SLOT_KEYBINDINGS` / `registerAgentSlotKeyHandlers` / `registerGlobalAgentSlotInputHandler` / `selectAgentSlot` | `AGENT_SLOT_KEYBINDINGS` / `register_agent_slot_key_handlers` / `register_global_agent_slot_input_handler` / `select_agent_slot` | `ui_tests::agent_slot_keybindings_select_active_agents_and_main`, `ui_tests::inactive_agent_slot_reports_status_without_switching` |
| `syncWorkingLoaderVisibility` / `createWorkingLoader` / `stopWorkingLoader` / `getWorkingLoaderMessage` | `sync_working_loader_visibility` / `create_working_loader` / `stop_working_loader` / `get_working_loader_message` | `ui_tests::thinking_and_tool_waiting_timers_update_the_working_loader`, `ui_tests::child_view_overrides_the_footer_session` |
| `getToolWaitingMessage` / `setDefaultWorkingMessage` / `setWorkingMessageForActiveTools` / `startToolWaitingTimer` / `stopToolWaitingTimerIfIdle` | `get_tool_waiting_message` / `set_default_working_message` / `set_working_message_for_active_tools` / `start_tool_waiting_timer` / `stop_tool_waiting_timer_if_idle` | `ui_tests::thinking_and_tool_waiting_timers_update_the_working_loader` |
| `getThinkingWorkingMessage` / `startThinkingTimer` / `stopThinkingTimer` / `addCompletedThinkingDurationBeforeToolCall` | `get_thinking_working_message` / `start_thinking_timer` / `stop_thinking_timer` / `add_completed_thinking_duration_before_tool_call` | `ui_tests::thinking_and_tool_waiting_timers_update_the_working_loader` |
| `startChildActivityTimer` / `stopChildActivityTimer` / `isSelectedChildWorking` | `start_child_activity_timer` / `stop_child_activity_timer` / `is_selected_child_working` | `ui_tests::child_view_overrides_the_footer_session` |
| `auto_retry_start` loader + `retryCountdown` / `stopRetryLoader` / retry escape handler | `AutoRetryStart` branch / `CountdownTimer` / `stop_retry_loader` / `retry_escape_handler` | `ui_tests::retry_loader_counts_down_and_reports_final_failure` |
| `compaction_start` loader + `formatCompactionStartLabel` / `autoCompactionSourceHint` / Escape -> `abortCompaction` | `CompactionStart` branch / `format_compaction_start_label` / `auto_compaction_escape_handler` / `auto_compaction_loader` / `auto_compaction_source_hint` | `ui_tests::compaction_start_renders_the_source_loader_and_binds_escape`, `ui_tests::compaction_labels_match_the_source` |
| `compaction_end` + `formatCompactionSourceLogMessage` / `formatCompactionFailureMessage` | `CompactionEnd` branch / `format_compaction_source_log_message` / `format_compaction_failure_message` | `ui_tests::compaction_end_reports_the_source_and_reason_specific_failures`, `ui_tests::compaction_labels_match_the_source` |
| `checkTmuxKeyboardSetup` | `check_tmux_keyboard_setup` / `check_tmux_keyboard_setup_with` | `tests/interactive_tmux_keyboard.rs` (8 cases: outside tmux, on/always, off, xterm, trimming, non-zero exit, missing format, 2 s timeout) |
| `handleHiddenMainSessionDisplayEvent` / `syncToolExecutionTrackingForHiddenMainEvent` | `handle_hidden_main_session_display_event` / `sync_tool_execution_tracking_for_hidden_main_event` | `ui_tests::agent_slot_keybindings_select_active_agents_and_main` (main events suppressed while viewing a child) |
| `subscribeToMultiAgentStore` / `poll` of store updates | `subscribe_to_multi_agent_store` / `poll_multi_agent_updates` | `ui_tests::agent_slot_keybindings_select_active_agents_and_main` |
| `openChildAgentView` / `watchSelectedAgentTranscript` / `scheduleSelectedAgentTranscriptReload` / `reloadSelectedAgentTranscript` / `retrySelectedAgentTranscriptReload` | `open_child_agent_view` / `watch_selected_agent_transcript` / `schedule_selected_agent_transcript_reload` / `reload_selected_agent_transcript` / `retry_selected_agent_transcript_reload` | `ui_tests::child_transcript_reloads_after_a_watched_file_change` |
| `renderSelectedAgentView` / `renderLiveAgentPlaceholder` / `findReadableAgentLogPath` / `readAgentLogPreview` | `render_selected_agent_view` / `render_live_agent_placeholder` / `find_readable_agent_log_path` / `read_agent_log_preview` | `ui_tests::child_view_overrides_the_footer_session`, `tests/interactive_child_view.rs` |
| `moveStreamingComponentToChat` / `discardStreamingComponent` | `move_streaming_component_to_chat` / `discard_streaming_component` | `ui_tests::message_end_moves_the_streaming_component_into_the_chat_container`, `ui_tests::fullscreen_message_start_renders_directly_in_the_chat_container` |
| `updateSelectedAgentSelectionWidgets` | `update_selected_agent_selection_widgets` | `ui_tests::agent_slot_keybindings_select_active_agents_and_main` |
| `renderSessionEntries` / `renderSessionItems` / `addCustomEntryToChat` / `getPendingToolStartedAt` | `render_session_entries` / `render_session_message` / `add_custom_entry_to_chat` / `get_pending_tool_started_at` | `ui_tests::render_session_entries_renders_persisted_custom_entries`, `ui_tests::render_session_entries_replays_tool_calls_and_results`, `ui_tests::entry_appended_renders_registered_custom_entries` |
| `entry_appended` branch | `AgentSessionEvent::EntryAppended` -> `add_custom_entry_to_chat` | `ui_tests::entry_appended_renders_registered_custom_entries` |
| `renderInitialMessages` / `renderProjectTrustWarningIfNeeded` | `render_initial_messages` / `render_project_trust_warning_if_needed` | `ui_tests::startup_renders_the_project_trust_warning` |
| `completedToolTimings` | `completed_tool_timings` | `ui_tests::render_session_entries_replays_tool_calls_and_results` |
| old-only cache-miss notices | removed, no common-pin counterpart | actual `message_end_ignores_removed_cache_notice_setting` RED→GREEN |
| `handleCtrlZ` / `suspendToBackground` | `handle_ctrl_z` / `suspend_to_background` | no direct test (sends SIGTSTP) |
| `detachRunningTool` + `app.tool.background` action + startup hint | `detach_running_tool` + `on_action("app.tool.background")` + `key_hint("app.tool.background", ...)` | `ui_tests::background_tool_keybinding_detaches_the_running_tool_and_shows_the_hint` |
| `registerSignalHandlers` / `unregisterSignalHandlers` / `shutdown({fromSignal})` / `restartProcess({fromSignal})` | `register_signal_handlers` / `unregister_signal_handlers` / `shutdown_from_signal` / `restart_from_signal` | `main_interactive::sigterm_shuts_down_interactive_mode_gracefully`, `main_interactive::sighup_restarts_interactive_mode_in_place` |
| `DEAD_TERMINAL_ERROR_CODES` / `isDeadTerminalError` (:345-354) | `is_dead_terminal_error` (raw errno `EIO`/`EPIPE`/`ENOTCONN`); thrown values go through `payload_is_dead_terminal_error` (only an `io::Error` payload carries a code) | `interactive_crash::dead_terminal_errors_are_eio_epipe_and_enotconn` (EIO/EPIPE/ENOTCONN true; ENOENT, code-less `other`, kind-only `BrokenPipe` false), `tests::only_io_error_payloads_can_be_dead_terminal_errors` |
| `emergencyTerminalExit` (:5561) | `emergency_terminal_exit` (sets `is_shutting_down`, unregisters handlers, `kill_tracked_detached_children`, exit 129, no `ui.stop()`) | `interactive_crash::dead_terminal_exits_129_without_restoring` (pty master closed -> stdin `EIO` -> exit 129, no crash banner) |
| `uncaughtCrash` (:5570-5600) | `uncaught_crash` + pure `crash_action` (dead terminal -> 129; already shutting down -> exit 1; else restore/report/exit 1); each `try { } catch {}` step runs under `uncaught::swallow_panics` | `tests::uncaught_crash_exits_129_for_a_dead_terminal_before_checking_shutdown`, `tests::uncaught_crash_exits_one_while_already_shutting_down`, `tests::uncaught_crash_restores_the_terminal_otherwise`, `uncaught::swallowed_panics_do_not_reach_the_handler_or_stop_later_steps`, `interactive_crash::uncaught_exception_restores_the_terminal_and_exits_one` (exit 1, `pi exiting due to uncaughtException:`, `\x1b[?2004l` / `\x1b[?25h` / `\x1b[<u` restore before the report) |
| `terminalErrorHandler` on `process.stdin/stdout/stderr` `error` (:5637-5646) | `terminal_error_action` + `terminal_error_handler`, fed by `handle_terminal_errors` from `Terminal::take_errors` (stdout write/flush failures and stdin read failures queued by `ProcessTerminal`) in the run loop | `tests::terminal_errors_route_dead_raw_mode_and_other_errors_separately` (EIO -> exit, Bun-style `setRawMode failed with errno:` -> ignore, ENOSPC / plain -> rethrow), pi-tui `terminal::write_failures_are_queued_for_the_stream_error_handler`, `terminal::successful_writes_queue_no_errors` |
| `process.prependListener("uncaughtException", ...)` (:5657) | `register_signal_handlers` installs the process crash handler (`uncaught::set_process_crash_handler`) that the panic hook (`install_uncaught_handler`) and therefore `throw_uncaught` run; `unregister_signal_handlers` clears it | `uncaught::process_crash_handler_receives_uncaught_panics`, `uncaught::process_crash_handler_receives_uncaught_throws`, `uncaught::clearing_the_process_crash_handler_restores_the_default_report`, `uncaught::set_process_crash_handler_reports_the_previous_handler` |
| `void this.restartProcess({ fromSignal: true })` rejection (:5623) | `restart_from_signal` -> `throw_uncaught` -> process crash handler -> `uncaught_crash` | `interactive_crash::sighup_restart_failure_restores_the_terminal_and_exits_one` (binary deleted before SIGHUP; exit 1, banner after restore) |
| Command-context `restart` -> `runtimeHost.restart({process: true})` | `UiRequest::RuntimeRestart` always passes `process: true` and returns the runtime error to the extension caller | `ui_tests::command_restart_always_restarts_the_process_and_propagates_errors` |

## Implemented branches

- Pure helpers: `quoteIfNeeded`, `formatFollowUpPreview`,
  `formatSessionTokenInfo`, `formatSessionCostInfo`,
  `getCompactExtensionLabels` (index stripping, parent disambiguation, npm/git
  package labels), `buildScopeGroups`, `formatScopeGroups`,
  `formatDiagnostics` (collision groups + other diagnostics), `formatCompactList`,
  `ExpandableText`.
- Startup: theme registration, fullscreen/regular layout, header (the
  background-tool hint is rendered through `key_hint`; the remaining header
  lines are the existing fixed labels), loaded
  resources (context, user rules, tools, skills, prompts, extensions, themes,
  diagnostics), initial messages, extension binding, subscription.
- Session events: agent/model-request lifecycle, user/assistant/custom message
  start/update/end, tool execution start/update/end, queue updates, compaction
  start/end, auto-retry start/end, provider stream retry, session info/thinking
  level changes, bash-message commits.
- Editor submission: slash-command dispatch, bash mode (`!`/`!!`), streaming
  steering, compaction queueing, plain prompts.
- Slash commands implemented: `/help`, `/hotkeys`, `/settings`, `/scoped-models`,
  `/model`, `/thinking`, `/theme`, `/session`, `/tools`, `/extensions`,
  `/changelog`, `/copy`, `/name`, `/unname`, `/compact`, `/reload`, `/export`,
  `/import`, `/cd`, `/clone`, `/new`, `/continue`, `/quit`, `/fork`, `/tree`,
  `/trust`, `/login`, `/logout`, `/resume`, extension commands, prompt templates.
- Key actions: clear/exit/suspend, thinking cycle/toggle, model cycle, tools
  expand, external editor, copy, follow-up, dequeue, new session, tree, fork,
  resume, and the agent switcher (`/agents`, `app.agent.select`).
- Multi-agent child view: `selectAgentViewFromBridge` selects a store view and
  renders the child transcript in the chat container (or a lifecycle
  placeholder when no transcript exists); `/agents` opens the ported
  `AgentSwitcherComponent`; the selection banner is mounted above the widgets;
  plain-text submissions steer the selected agent; Escape requests cancellation;
  `steering_message_queued` wakes waiting agents. Proof:
  `interactive_child_view` (transcript boundary), `interactive_components`
  (banner/switcher), `agents_core_runtime` (async selector routing).
- Tool timers: `ToolExecutionStart.startedAt` / `ToolExecutionEnd.finishedAt`
  epoch milliseconds flow into `ToolExecutionComponent`; elapsed text is
  `finishedAt ?? Date.now() - startedAt` (`pi_js_compat::date::js_now_millis`).
- Share: `gh auth status` gate, session HTML export, cancellable
  `BorderedLoader`, `gh gist create --public=false`, abort kills the child,
  gist id parsing and viewer URL status.
- Startup: fresh-install version recording plus install telemetry through the
  configured `Fetch`; new-entry changelog rendering with collapse setting.
- Prompt activity: `bash`, `branch-summary`, `compaction`, `retry` and
  `summary-materialization` sources drive the working editor prompt.
- Compaction UI: `compaction_start` sets terminal progress, stores the source
  hint, replaces the editor Escape handler with `session.abortCompaction()`, and
  renders a `Loader` whose label is
  `formatCompactionStartLabel(reason, sourceHint, "(<interrupt key> to cancel)")`
  (`Compacting context` for manual, `Auto-compacting` otherwise, plus the
  OpenAI-remote/local suffix). `compaction_end` clears progress, restores the
  Escape handler, stops the loader, and reports either the cancellation status,
  `formatCompactionSourceLogMessage(result.source)`, or the reason-specific
  `formatCompactionFailureMessage` (manual failures use `showError`; automatic
  failures append a spacer and an error text to the chat container).
- tmux: `run()` starts `checkTmuxKeyboardSetup()` asynchronously and shows its
  warning. The check queries `extended-keys` and `extended-keys-format` with a
  2 s timeout, ignores failures, and warns only for a non-`on`/`always` value or
  the `xterm` format.
- Regular-mode render regions: transcript-tail flow layout and status region
  receive `place`; the editor layout callback positions/clears the working
  prompt cell.
- Fullscreen: scrollbar thumb uses the `scrollbarThumb` theme background.
- Extension UI: select, confirm, input, editor, notify, requestRender,
  onTerminalInput, setStatus, working message/visibility/indicator,
  hidden-thinking label, string widgets, setTitle, paste/set/get editor text,
  getAllThemes/getTheme/setTheme, tools-expanded, footer/header/custom
  component factories, runtime new-session/fork/tree/switch/reload/restart
  requests, abort and shutdown handlers.

## Process-wide crash restoration

`uncaught::ProcessCrashHandler` is an `Arc<dyn Fn + Send + Sync>` published
under a process-wide mutex. The panic hook clones it before dispatch; clearing
it restores the default report on every thread. Shutdown uses a shared atomic;
dead-terminal errors still take precedence over already-shutting-down exit 1.
Signal task abort handles are Send-able and cleared before child teardown.

`ProcessTerminal::crash_restore` shares saved termios and the exact protocol
stop bytes with `TerminalRestore`. The UI publishes its stop prefix/suffix at
start, color-notification changes, and completed renders. Regular cursor
placement and fullscreen mouse/autowrap/image cleanup and transcript output
share their byte builders with normal `ui.stop()`. The hook restores these
bytes and termios directly, without accessing `Rc`/`RefCell` UI state or waiting
for the UI event loop. Its restore lock excludes subsequent terminal writes
through the final report and process exit. This avoids a timeout/fallback path
when the UI thread is busy. Renderer snapshots describe the committed frame;
terminal protocol state is refreshed as negotiation/progress/drain changes it.

Proof: `interactive_crash::{extension_thread_panic_restores_the_terminal,
blocking_pool_panic_restores_the_terminal,
tokio_worker_panic_restores_the_fullscreen_terminal}` drive real PTYs, checking
exit 1, original termios, banner/error and restoration before the banner.
`worker_restore_matches_normal_ui_stop_bytes` additionally compares complete
normal-stop and worker-restore byte streams in both renderer modes, with
progress and color notifications enabled and modifyOtherKeys negotiated.
The PTY harness captures immutable initial termios immediately after `openpty`,
before spawning the child; worker-crash and stop-output tests use that baseline.
The stop-output regression waits for `STOP_READY` (after `ui.start()`) before
fetching the stored baseline, asserts live raw termios differs, then asserts
final live termios equals the baseline in all four regular/fullscreen ×
normal/worker cases. Sampling live termios after that handshake deterministically
fails the raw-versus-baseline assertion (RED); pre-spawn capture removes the
baseline race. Proof is handshake ordering, not stress reruns; no production
restoration change is involved.
`dead_terminal_error_on_a_worker_exits_129_without_restoring` proves the
process-wide emergency branch. `uncaught` tests prove cross-thread dispatch
and handler removal from another thread.
The triggers exist only in ignored integration-test children; production has
no test environment switch. Existing UI-thread, dead-terminal and SIGHUP
failure tests remain required. OS/process behavior is tested at that boundary,
not through a deterministic TypeScript oracle.

## Runtime loop

`run` binds callbacks, subscribes to session events, calls `init`, processes the
claimed control message, then prompts initial message/messages and loops:
`process_events` -> `flush_pending` -> border color -> UI requests -> queued
actions, breaking when shutdown is requested, then `shutdown`.

`shutdown` mirrors the source: disable theme auto-sync, drain terminal input,
`stop()`, dispose the runtime, then print
`To resume this session: <formatResumeCommand>` when the session is persisted
and its file exists. `stop()` disposes the active selector, clears terminal
progress, stops the loader and child-activity timer, clears the child view,
disables theme auto-sync, unsubscribes extension terminal listeners, disposes
custom/default footer components plus the footer data provider, unsubscribes
agent + multi-agent updates, cancels partial-update rendering (the Rust loop
holds the throttle in `tokio::select!`), drops active bash and pending tool
components, closes the completion notification, cleans clipboard temp files,
then for an initialized TUI runs `stopInteractiveTui` and unregisters signal
handlers. `transcript` exit output leaves fullscreen, switches to the regular
renderer and re-renders; `resume-hint` keeps the fullscreen renderer with
`preserveScreen`.

## Gaps (explicit)

No remaining crash-handler gap. The coverage row stays `partial`: custom-editor
initial-history replay differs from `populateHistory` (below), and whole-mode
TypeScript differential coverage remains unavailable. This slice does not
approve or resolve those pre-existing discrepancies.

The fullscreen dock and transcript-tail layout are placed by the layout root
in the source as well; the fullscreen renderer intentionally has no separate
render-region callbacks.

## populateHistory

| Source | Rust | Test |
|---|---|---|
| `renderInitialMessages` `populateHistory: true`; `addMessageToChat` user branch skips `inputSource === "extension"` and skill blocks; `addRenderedMessageToEditorHistory` (default editor without persistence, custom editor `addToHistory`) | `render_initial_messages` → `render_session_entries_with_cwd(.., true)`, `add_message_to_chat(.., populate_history)`, `add_rendered_message_to_editor_history` | `ui_tests::initial_render_adds_interactive_user_messages_to_editor_history` |

## Discrepancies

- `console.error(error)` prints a Node stack; the Rust report is
  `<label>: <message>` (`format_uncaught`) with no stack frames (no JS stack
  exists). Rethrown terminal stream errors use Node's `<syscall> <CODE>`
  message (`terminal_error_message`, tested by
  `interactive_crash::rethrown_terminal_errors_use_the_node_stream_message`).
- `SelectAgentView` is async (`BoxFuture<Option<bool>>`) because the TUI lives on
  a `LocalSet` thread; the bridge selector sends a `UiRequest` and awaits the
  UI-thread result instead of calling the mode synchronously.
- **OPEN viewed-child UI target:** the session-level resolver bound in `main.rs` serves extension command contexts; direct UI model candidates/set_model still target the main session. Frozen `resolveViewedSessionTarget` :651–659 requires the selected live child or an explicit throw. This is a separate queued integration slice, not covered by main-session host proof.
- `toLocaleString()` token counts are rendered with `en-US` grouping. The
  TypeScript output depends on the host locale; this port pins `en-US`
  (approved deviation, user decision 2026-10-19).
- `setInterval` timers and the retry `CountdownTimer` are deadline-driven:
  `poll_timers` runs from the TUI loop and advances thinking, tool-waiting,
  child-activity and retry messages once per elapsed second. Observable text
  matches the source; the tick count is not observable.
- `fs.watch` events cross from the notify thread through an `mpsc` channel and
  are drained by `poll_child_view_events`; the 50 ms reload debounce, the
  3-retry schedule and the 5 s watch retry match the source.
- Session replay (`renderSessionEntries`) renders context entries in order,
  including custom entries through their registered entry renderers, and
  reconstructs assistant tool-call components matched with tool results. This
  is shared by startup, child views and the transcript rebuild.
- The share flow accepts an explicit `gh` program path
  (`handle_share_command_with_program`) so tests can run a recording script
  without mutating the process `PATH`; production passes `"gh"` exactly like
  the source.

## Differential

Not implemented for the whole mode: driving the real TypeScript
`InteractiveMode` requires a TTY/faux runtime harness that this slice did not
build, and importing `interactive-mode.ts` under standalone `tsx` fails in the
pinned worktree (`jiti` does not export `./static`). Deterministic compaction
helpers are covered by unit tests ported from
`packages/coding-agent/test/interactive-mode-compaction.test.ts`; the
interactive behavior tests drive the real Rust mode in a capture terminal with
the faux provider. No whole-mode differential is an approved deviation
(user decision 2026-10-20): the behavior tests below are the proof.

Behavior tests drive the real mode through a capture terminal: startup,
prompt submission, bash mode, settings, scoped models, trust, sandbox,
approval, fork, tree, resume, login, logout, changelog notices, telemetry,
prompt activity, editor layout, subscription warning, easter eggs, share,
agent slots, working-loader timers, retry countdown, child footer override,
MessageEnd placement (regular and fullscreen), notify-based child transcript
reloads, and the extension autocomplete/editor factories.
`main_interactive` drives `/share` dispatch through the real binary under a
pty with a fake `gh` on `PATH`.

## Startup extension inventory lock regression

`show_loaded_resources` copies `path` and `source_info` through one registration
guard. Two `extension.read()` temporaries in the same tuple retain the first
mutex guard while acquiring the second, blocking the UI thread in `init`.
Pinned TS `interactive-mode.ts:2158–2166` directly reads both fields; it does
not block or require a test-only startup bypass.

Introduced by `ea57863f`, first merged to master in `04f51bab`: all three tests
below pass at its first parent `5d2454df` and time out at that merge. Temporary
instrumentation at `b5cfa925` reaches the path read, then blocks acquiring
`source_info` in each case. Regression proofs drive the real capture-terminal
startup, then custom-entry rendering or command restart:

| Capability | Behavioral proof |
| --- | --- |
| Startup with a visible extension lists its path | `ui_tests::entry_appended_renders_registered_custom_entries` (also asserts rendered resource inventory) |
| Live custom entries render after startup | `ui_tests::entry_appended_renders_registered_custom_entries` |
| Persisted custom entries replay after startup | `ui_tests::render_session_entries_renders_persisted_custom_entries` |
| Extension restart reaches process restarter and propagates failure | `ui_tests::command_restart_always_restarts_the_process_and_propagates_errors` |

Same-receiver repeated-read audit found no other overlapping tuple reads in
production Rust. Resource-loader filter reads end before loop-body reads;
runner candidates are separate functions. No fixture bypass, source semantic
deviation, or new gap.

## v15 name paths (approved 2026-10-20)

`c248b0067` name-only changes: selector ordering uses each loaded metadata-backed `SessionInfo.name`,
non-current rename calls `write_session_name` directly without opening/appending JSONL, and
`/name`/`/unname` use the fallible shared-manager name methods. Tests: picker ordering cases,
`agent_session` name-state event case, RPC metadata-only name process test. Other interactive
shutdown/crash changes remain outside this slice.

## Extension editor host / component focus

TS `Editor` uses `tui.createFixedCell`, `terminal.rows`, `requestRender` (editor.ts:375,805); `TUI.setFocusInternal` clears/sets optional `focused` (tui.ts:690–697). Rust factory TUI backends resolve the live owning mode and allocate real renderer fixed cells in both screen modes. ExtensionComponentAdapter and AnyEditorHandle forward optional focus. Proof: `extension_ui_factory_builds_real_custom_editor`, `extension_ui_overlay_focus_emits_cursor_and_restores_editor`. No whole-mode differential, per approved capture-terminal proof policy.

The overlay example also requires source `showExtensionCustom` semantics (interactive-mode.ts:3485–3528): resolve optional component width, hide overlay on first done, resolve once, dispose once, skip mounting after synchronous done. Shared component references are weak in the done callback to avoid ownership cycles; disposal remains outside RefCell borrowing. Tests: `extension_ui_overlay_uses_component_width_and_closes_once` drives Enter through the real TUI; explicit-options/synchronous-close test covers override and early done. The subsequent [core UI overlay slice](core-ui-overlay.md) supplies shared/local `onHandle` delivery, nested overlays, focus queries, and UI-local component ownership.

## Q18 native UTF-16 integration (2026-10-22)

`InteractiveModeOptions`, UI text/input/rendering, error presentation and editor/history content now retain JsString. Selector/command matching is unit-native; paths encode only for filesystem access or actual output. The provider-scoped integration `lossless_sdk_boundaries` passed 2/2 real sandbox tests at44a5d91d; this does not establish coding-agent runtime proof. `interactive6` at71cbce6e records 87 total library errors, four owned primary locations, with external accessor/SQL-contract blockers still open.

## Resource-display homedir propagation (2026-10-23)

Formatting paths, compact labels/segments, scope groups and diagnostics propagate shared node_os::homedir failures through ResourceDisplayResult to show_loaded_resources. Helpers never panic. Frozen caller audit: init/bindExtensions :2138 and selectAgentView(main) :3291 have no catch; their native initialization/public bridge boundaries call shared throw_native_uncaught. New-session callback follows the same existing uncaught boundary. Frozen /reload :7174–7221 catches and displays `Reload failed: ${error.message}`; native /reload now calls show_loaded_resources inside its caught success path, matching the missing frozen :7197 call.

`interactive_crash::resource_home::reload_catches_resource_display_homedir_error_without_crashing` runs a real PTY mode/runtime with faux provider and a ResourceLoader fault wrapper. Successful loader reload arms a temporary NSS-failure marker only when the resource-display-only rules getter is read; it therefore bypasses earlier config/package path lookups and exercises formatting → show_loaded_resources → /reload catch. RED (`resource-display-home-red.log`) showed the old handler only reporting “Reloaded” and never invoking resource display. GREEN (`resource-display-home-green.log`) shows the exact EIO `Reload failed: ...` row, no crash banner, and normal exit 0. Private formatting/crash helpers: 21 focused tests GREEN (`resource-display-format-tests.log`). No public helper visibility change or fallible Component API added. Remaining /reload theme/editor/autocomplete/runtimeHost UI-refresh differences stay for the whole-mode audit.

## Exact command dispatch

[Contract](../../specs/interactive-command-dispatch.md). Frozen `setupEditorSubmitHandler` :3969–4196 uses exact command strings except the seven explicit argument forms (`model`, `export`, `import`, `cd`, `name`, `compact`, `login`). Native name/argument splitting formerly consumed `/new extra` as `/new`, replacing the session instead of forwarding the text (RED `interactive-command-dispatch-red.log`). Native now preserves source exactness, removes old-only `/help`, `/thinking`, `/theme`, `/daxnuts` dispatch and deletes the old thinking/theme command-only selectors. The daxnuts easter-egg helper remains because frozen model-selection callbacks still call it.

Actual frozen `setupEditorSubmitHandler` is invoked on a fresh receiver per request (`interactive-command-dispatch-oracle.mts`, receiver/defaultEditor inside the request loop). Native uses a fresh real capture-terminal mode per case with the same `onInputCallback` observation boundary, comparing all 27 literal inputs and unchanged session identity. GREEN `interactive-command-dispatch-green.log`. Forwarding is deliberately not asserted as persisted-user-message success: frozen `agent-session.ts:3041–3044` rejects unknown slash names before `_promptTurn`; its known-name check is :3223–3230.

### Sequence classification: source-equivalent rejection

The original accumulated-session test reset one faux response before each of these submissions and compared persisted user messages:

```text
/new extra → /settings extra → /scoped-models extra → /share extra → /copy extra → /unname extra → /session extra → /tools extra → /extensions extra → /changelog extra → /hotkeys extra → /fork extra → /clone extra → /tree extra → /trust extra → /logout extra → /continue extra → /reload extra → /arminsayshi extra
```

At the last input the latest persisted user message stayed `/reload extra`; native chat reports `Unknown slash command: /arminsayshi`, streaming/compacting false. Fresh mode per case reproduced that rejection, falsifying accumulated-state loss. Diagnostic `interactive-command-dispatch-diagnostic.log`; the initial oracle observed callback forwarding while native asserted later persistence, so that comparison mixed stages. The follow-up `original_literal_command_sequence_matches_frozen_prompt_validation` executes the exact 19-input sequence against actual frozen editor-submit, submitMainLoopInput, AgentSession.prompt and its working-directory/known-command validation methods. Only the post-validation turn and sequential lock/coordination hooks are fixture seams; no provider/network is used. Native drives the real mode/session with faux responses. Every cumulative accepted-input/error frame matches: 18 persisted user inputs, then `Unknown slash command: /arminsayshi`. GREEN `interactive-command-sequence-green.log`. **Closed as a mismatched test observation stage, not a native input-loss bug.** The dispatcher forwarding proof remains separate from session acceptance. Whole-mode TS runtime parity is not inferred.

## /model main-session host

Frozen `handleModelCommand` :5929–5951 uses `findExactModelReferenceMatch` via candidate refresh/catch :5954–5971; non-exact or absent queries use the actual `showModelSelector` :6121–6152. Native old substring auto-selection and generic ExtensionSelector host are deleted. The actual ModelSelectorComponent mounts/focuses in the editor region, retains the initial search, and restores the editor on selection/cancel; post-selection status/warning/easter behavior is shared by exact-reference and widget paths. Footer provider count uses the same candidate authority rather than another refresh/catch policy.

`model_command_uses_exact_references_and_the_frozen_model_selector` compares actual frozen controller methods + actual ModelSelector widget to the real native mode. Eleven cases: missing/empty query, non-exact substring, id/provider-id/case-insensitive exact references, display-name search, no match, Enter, Escape, and arrow+Enter. It asserts exact UTF-16 widget rows, selected model id, and status rows. RED `interactive-model-command-red.log` showed the generic selector instead of the frozen widget; GREEN `interactive-model-command-green.log`. Source receiver supplies a deterministic registry/model setter but executes real candidate/exact-reference/controller/widget logic; this is not a whole-mode TS runtime differential. Viewed-child targeting and model-select key binding remain open, separately scoped above.
