# interactive-mode (`coding-agent/src/modes/interactive/interactive-mode.ts`)

Ported to `crates/pi-coding-agent/src/modes/interactive/interactive_mode.rs`.

Status: **partial**. The mode core (construction, TUI layout, startup resources,
session event rendering, editor submission, key handlers, slash commands,
extension UI dialogs, selector components, login/logout, suspend and clipboard
paste) is implemented and behavior-tested. Multi-agent child transcripts and
the share viewer remain explicit gaps below.

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
| `handleLoginCommand` / `showLoginAuthTypeSelector` / `showLoginProviderSelector` / `startProviderLogin` / `showLoginDialog` / `showApiKeyLoginDialog` / `completeProviderAuthentication` | `handle_login_command` / `show_login_auth_type_selector` / `show_login_provider_selector` / `start_provider_login` / `show_login_dialog` / `show_api_key_login_dialog` / `complete_provider_authentication` | `ui_tests::login_selector_opens_the_ambient_auth_dialog` |
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
  resume, and the agent switcher (`/agents`, `app.agent.select`).
- Multi-agent child view: `selectAgentViewFromBridge` selects a store view and
  renders the child transcript in the chat container (or a lifecycle
  placeholder when no transcript exists); `/agents` opens the ported
  `AgentSwitcherComponent`; the selection banner is mounted above the widgets;
  plain-text submissions steer the selected agent; Escape requests cancellation;
  `steering_message_queued` wakes waiting agents. Proof:
  `interactive_child_view` (transcript boundary), `interactive_components`
  (banner/switcher), `agents_core_runtime` (async selector routing).
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

- Agent slots (1-9 selection bindings) are not wired; the switcher covers agent
  selection.
- Share viewer (`/share`).
- Startup changelog rendering and install telemetry network call.
- Prompt-activity counters (`setPromptActivity`) used by the tree
  compaction-summary loaders; loaders and escape cancellation are implemented.
- Fullscreen render regions/scroll view: the fullscreen renderer is selected
  and mounted, but scroll-region layout callbacks are no-ops.
- Anthropic subscription-auth warning and the daxnuts easter egg hooks in
  `completeProviderAuthentication`.

## Discrepancies

- Child transcript updates use a metadata poll on the existing TUI tick instead
  of `fs.watch` plus a debounce timer; the visible reload behavior is the same.
- `SelectAgentView` is async (`BoxFuture<Option<bool>>`) because the TUI lives on
  a `LocalSet` thread; the bridge selector sends a `UiRequest` and awaits the
  UI-thread result instead of calling the mode synchronously.
- The child view does not override the footer session (model/token display);
  the footer keeps showing the main session.
- `bindInteractiveModeSessionMutationTargetResolver` has no Rust analogue: the
  session-level resolver bound in `main.rs` already resolves the selected child
  through the shared store and runtime handles, so `/model` and `/effort`
  target the viewed child without a second resolver.
- `toLocaleString()` token counts are rendered with `en-US` grouping. The
  TypeScript output depends on the host locale; this port pins `en-US`.
- Tool elapsed timers use `Instant::now()` when the event carries an epoch
  timestamp, because `Instant` has no epoch mapping. Elapsed values are
  therefore relative to event receipt rather than the reported start.
- `MessageEnd` updates the component created at `MessageStart` in place; the
  source moves the component from the transcript tail into the chat container.
  Rendering is equivalent in regular mode.
- Ambient API-key providers are detected by attempting the login dialog and
  falling back to the ambient info dialog when the runtime reports
  `"<provider> does not support api_key login"`. The Rust provider-auth traits
  expose `login()` but no side-effect-free availability query, unlike the
  TypeScript method object.
- `AgentSession::update_compaction_summary` logs failures instead of returning
  them (existing agent-session gap), so the compaction-summary edit path cannot
  surface update errors.

## Differential

Not implemented for the whole mode: driving the real TypeScript
`InteractiveMode` requires a TTY/faux runtime harness that this slice did not
build. Deterministic helpers are covered by unit tests; the interactive
behavior tests drive the real Rust mode in a capture terminal with the faux
provider. This is recorded as a coverage gap in `coverage.md`.

Behavior tests drive the real mode through a capture terminal: startup,
prompt submission, bash mode, settings, scoped models, trust, sandbox,
approval, fork, tree, resume, login and logout.
