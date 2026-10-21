# interactive-mode (`coding-agent/src/modes/interactive/interactive-mode.ts`)

Ported to `crates/pi-coding-agent/src/modes/interactive/interactive_mode.rs`.

Status: **done** for this module. The mode core (construction, TUI layout,
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
| `bindInteractiveModeSessionMutationTargetResolver` | not ported (see Discrepancies) | session-level resolver in `main.rs` |
| `stop(fullscreenExitOutput = getFullscreenExitOutput())` / `stopInteractiveTui` | `InteractiveMode::stop` / `stop_with_output` / `stop_interactive_tui` | `ui_tests::stop_renders_the_transcript_when_leaving_fullscreen`, `ui_tests::stop_with_resume_hint_keeps_the_fullscreen_renderer`, `ui_tests::stop_clears_terminal_progress_and_disposes_footer_components`, `PI_STARTUP_BENCHMARK` path in `main_runner` |
| `processControlMessage` | `process_control_message` | control-message branch of `run`; `IncomingControlMessage` completion/failure |
| `InteractiveTuiHandle` dispatch | `InteractiveTuiHandle::renderer()` clones the active renderer | pty interactive test (input callbacks request renders) |
| `formatResumeCommand` | `format_resume_command` | unit tests for quoting; `main_interactive::quit_prints_the_resume_command` covers the TTY branch and the `shutdown` output line |
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
| `maybeShowCacheMissNotice` / `addCacheMissNotice` | `maybe_show_cache_miss_notice` / `add_cache_miss_notice` | `ui_tests::message_end_shows_a_cache_miss_notice` |
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
- `bindInteractiveModeSessionMutationTargetResolver` has no Rust analogue: the
  session-level resolver bound in `main.rs` already resolves the selected child
  through the shared store and runtime handles, so `/model` and `/effort`
  target the viewed child without a second resolver.
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

## v15 name paths (approved 2026-10-20)

`c248b0067` name-only changes: selector ordering uses each loaded metadata-backed `SessionInfo.name`,
non-current rename calls `write_session_name` directly without opening/appending JSONL, and
`/name`/`/unname` use the fallible shared-manager name methods. Tests: picker ordering cases,
`agent_session` name-state event case, RPC metadata-only name process test. Other interactive
shutdown/crash changes remain outside this slice.

## Extension editor host / component focus

TS `Editor` uses `tui.createFixedCell`, `terminal.rows`, `requestRender` (editor.ts:375,805); `TUI.setFocusInternal` clears/sets optional `focused` (tui.ts:690–697). Rust factory TUI backends resolve the live owning mode and allocate real renderer fixed cells in both screen modes. ExtensionComponentAdapter and AnyEditorHandle forward optional focus. Proof: `extension_ui_factory_builds_real_custom_editor`, `extension_ui_overlay_focus_emits_cursor_and_restores_editor`. No whole-mode differential, per approved capture-terminal proof policy.

The overlay example also requires source `showExtensionCustom` semantics (interactive-mode.ts:3485–3528): resolve optional component width, hide overlay on first done, resolve once, dispose once, skip mounting after synchronous done. Shared component references are weak in the done callback to avoid ownership cycles; disposal remains outside RefCell borrowing. Tests: `extension_ui_overlay_uses_component_width_and_closes_once` drives Enter through the real TUI; explicit-options/synchronous-close test covers override and early done. The subsequent [core UI overlay slice](core-ui-overlay.md) supplies shared/local `onHandle` delivery, nested overlays, focus queries, and UI-local component ownership.
