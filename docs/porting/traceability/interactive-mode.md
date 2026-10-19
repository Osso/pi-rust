# interactive-mode (`coding-agent/src/modes/interactive/interactive-mode.ts`)

Ported to `crates/pi-coding-agent/src/modes/interactive/interactive_mode.rs`.

Status: **partial**. The mode core (construction, TUI layout, startup resources,
session event rendering, editor submission, key handlers, slash commands,
extension UI dialogs, selector components, login/logout, suspend, clipboard
paste, share viewer, startup changelog/telemetry, prompt activity, render-region
layout callbacks, subscription warning and easter eggs) is implemented and
behavior-tested. Multi-agent child transcripts remain an explicit gap below.

## Exported symbols

| Source symbol | Rust item | Tests |
| --- | --- | --- |
| `InteractiveMode` | `InteractiveMode` | `ui_tests::renders_startup_screen_and_prompt_submission`, `ui_tests::bash_mode_executes_and_renders_command_output` |
| `InteractiveModeOptions` | `InteractiveModeOptions` | construction in `ui_tests::build_mode` |
| `createInteractiveTui` | `create_interactive_tui` | exercised by `build_mode` (regular); fullscreen renderer construction covered by `pi-tui` renderer tests |
| `createInteractiveTuiReference` | `InteractiveTuiHandle` (stable renderer handle) | construction/renderer swap used by components; no direct test |
| `bindInteractiveModeSessionMutationTargetResolver` | not ported | gap: no `ViewedSessionMutationTarget` API |
| `stop()` | `InteractiveMode::stop` | `PI_STARTUP_BENCHMARK` path in `main_runner` |
| `processControlMessage` | `process_control_message` | control-message branch of `run`; `IncomingControlMessage` completion/failure |
| `InteractiveTuiHandle` dispatch | `InteractiveTuiHandle::renderer()` clones the active renderer | pty interactive test (input callbacks request renders) |
| `formatResumeCommand` | `format_resume_command` | unit tests for quoting; TTY-dependent branch not covered |
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
| `handleShareCommand` | `handle_share_command` / `handle_share_command_with_program` | `ui_tests::share_command_creates_a_gist_and_reports_the_viewer_url`, `main_interactive::share_command_reports_a_missing_gh_login` |
| `getChangelogForDisplay` / `reportInstallTelemetry` / `showStartupNoticesIfNeeded` | `get_changelog_for_display` / `changelog_display_for` / `report_install_telemetry` / `show_startup_notices_if_needed` | `ui_tests::changelog_display_returns_only_entries_newer_than_the_seen_version`, `ui_tests::startup_notices_render_new_changelog_entries`, `ui_tests::install_telemetry_reports_the_version_through_the_configured_fetch` |
| `setPromptActivity` / `syncWorkingEditorState` | `set_prompt_activity` / `sync_working_editor_state` | `ui_tests::prompt_activity_animates_the_prompt_until_the_source_finishes` |
| `handleEditorLayout` / render-region `place` callbacks | `rebuild_root_compositor` layout callbacks | `ui_tests::editor_layout_callback_positions_the_working_editor` |
| `applyFullscreenScrollbarSetting` / `scrollbarStyle` | `build_fullscreen_layout` `scrollbar_style` | `ui_tests::fullscreen_scroll_view_uses_the_scrollbar_thumb_background` |
| `handleArminSaysHi` / `handleDementedDelves` / `handleDaxnuts` / `checkDaxnutsEasterEgg` | `handle_armin_says_hi` / `handle_demented_delves` / `handle_daxnuts` / `check_daxnuts_easter_egg` | `ui_tests::daxnuts_easter_egg_triggers_for_opencode_kimi_models` |
| `showOAuthSelector` (logout) | `show_oauth_selector(true)` | `ui_tests::logout_selector_removes_the_stored_api_key` |
| `setEditorComponent` / `getEditorComponent` / `addAutocompleteProvider` | `set_custom_editor_component` / `UiRequest::GetEditorComponent` / `add_autocomplete_provider` | `ui_tests::extension_select_dialog_resolves_and_restores_editor` plus component tests in `tests/differential_interactive_components.rs` |
| `handleCtrlZ` / `suspendToBackground` | `handle_ctrl_z` / `suspend_to_background` | no direct test (sends SIGTSTP) |

## Implemented branches

- Pure helpers: `quoteIfNeeded`, `formatFollowUpPreview`,
  `formatSessionTokenInfo`, `formatSessionCostInfo`,
  `getCompactExtensionLabels` (index stripping, parent disambiguation, npm/git
  package labels), `buildScopeGroups`, `formatScopeGroups`,
  `formatDiagnostics` (collision groups + other diagnostics), `formatCompactList`,
  `ExpandableText`.
- Startup: theme registration, fullscreen/regular layout, header, loaded
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
  resume; the agent-select action remains a gap message.
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

## Runtime loop

`run` binds callbacks, subscribes to session events, calls `init`, processes the
claimed control message, then prompts initial message/messages and loops:
`process_events` -> `flush_pending` -> border color -> UI requests -> queued
actions, breaking when shutdown is requested, then `shutdown`.

## Gaps (explicit)

- Multi-agent child view: `selectAgentView`, child transcript watching/reload,
  agent slots, agent switcher, banner updates, `steerMultiAgent`,
  `cancelMultiAgent`. Requires the child `SessionManager`/transcript API.
- Working loader (`syncWorkingLoaderVisibility`, thinking/tool waiting timers,
  retry countdown) is not ported; prompt activity currently drives the editor
  prompt animation only.
- `bindInteractiveModeSessionMutationTargetResolver` / viewed-session target
  redirection.
- Fullscreen scroll view `scrollbar_style` is wired; region placement for the
  fullscreen dock is handled by the layout root rather than render regions
  (the source also uses the layout root).

## Discrepancies

- `toLocaleString()` token counts are rendered with `en-US` grouping. The
  TypeScript output depends on the host locale; this port pins `en-US`.
- `MessageEnd` updates the component created at `MessageStart` in place; the
  source moves the component from the transcript tail into the chat container.
  Rendering is equivalent in regular mode.
- The share flow accepts an explicit `gh` program path
  (`handle_share_command_with_program`) so tests can run a recording script
  without mutating the process `PATH`; production passes `"gh"` exactly like
  the source.

## Differential

Not implemented for the whole mode: driving the real TypeScript
`InteractiveMode` requires a TTY/faux runtime harness that this slice did not
build. Deterministic helpers are covered by unit tests; the interactive
behavior tests drive the real Rust mode in a capture terminal with the faux
provider. This is recorded as a coverage gap in `coverage.md`.

Behavior tests drive the real mode through a capture terminal: startup,
prompt submission, bash mode, settings, scoped models, trust, sandbox,
approval, fork, tree, resume, login, logout, changelog notices, telemetry,
prompt activity, editor layout, subscription warning, easter eggs and share.
`main_interactive` drives `/share` dispatch through the real binary under a
pty with a fake `gh` on `PATH`.
