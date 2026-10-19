# interactive-mode (`coding-agent/src/modes/interactive/interactive-mode.ts`)

Ported to `crates/pi-coding-agent/src/modes/interactive/interactive_mode.rs`.

Status: **partial**. The mode core (construction, TUI layout, startup resources,
session event rendering, editor submission, key handlers, core slash commands,
extension UI dialogs) is implemented and behavior-tested. Selector-heavy
features that depend on unshared `Rc<RefCell<SettingsManager>>` ownership or on
multi-agent child transcripts are explicit gaps below.

## Exported symbols

| Source symbol | Rust item | Tests |
| --- | --- | --- |
| `InteractiveMode` | `InteractiveMode` | `ui_tests::renders_startup_screen_and_prompt_submission`, `ui_tests::bash_mode_executes_and_renders_command_output` |
| `InteractiveModeOptions` | `InteractiveModeOptions` | construction in `ui_tests::build_mode` |
| `createInteractiveTui` | `create_interactive_tui` | exercised by `build_mode` (regular); fullscreen renderer construction covered by `pi-tui` renderer tests |
| `createInteractiveTuiReference` | `InteractiveTuiHandle` (stable renderer handle) | construction/renderer swap used by components; no direct test |
| `bindInteractiveModeSessionMutationTargetResolver` | not ported | gap: no `ViewedSessionMutationTarget` API |
| `formatResumeCommand` | `format_resume_command` | unit tests for quoting; TTY-dependent branch not covered |
| `ExtensionUIContext` implementation | `InteractiveUiContext` | `ui_tests::extension_select_dialog_resolves_and_restores_editor` (select); confirm/input/editor share the same broker |

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
- Slash commands implemented: `/help`, `/hotkeys`, `/model`, `/thinking`,
  `/theme`, `/session`, `/tools`, `/extensions`, `/changelog`, `/copy`,
  `/name`, `/unname`, `/compact`, `/reload`, `/export`, `/import`, `/cd`,
  `/clone`, `/new`, `/continue`, `/quit`, extension commands, prompt templates.
- Key actions: clear/exit/suspend message, thinking cycle/toggle, model cycle,
  tools expand, external editor, copy, follow-up, dequeue, new session,
  tree/fork/resume/agent-select gap messages.
- Extension UI: select, confirm, input, editor, notify, requestRender,
  onTerminalInput, setStatus, working message/visibility/indicator,
  hidden-thinking label, string widgets, setTitle, paste/set/get editor text,
  getAllThemes/getTheme/setTheme, tools-expanded, footer/header/custom
  component factories, runtime new-session/fork/tree/switch/reload/restart
  requests, abort and shutdown handlers.

## Gaps (explicit)

- Multi-agent child view: `selectAgentView`, child transcript watching/reload,
  agent slots, agent switcher, banner updates, `steerMultiAgent`,
  `cancelMultiAgent`. Requires the child `SessionManager`/transcript API.
- Selector components that require `Rc<RefCell<SettingsManager>>` sharing:
  `/settings`, `/scoped-models`, trust, sandbox, login/logout, model selector
  (search UI), session selector, tree selector, user-message/fork selector.
  The mode shows an explicit "not ported yet" status for these commands.
- `setEditorComponent` / `getEditorComponent` (custom editor factories).
- `addAutocompleteProvider` (wrapper factories).
- Startup changelog rendering and install telemetry network call.
- Clipboard image paste (`Ctrl+V`), right-click paste, suspend (`Ctrl+Z`),
  signal handlers, process restart.
- Fullscreen render regions/scroll view: the fullscreen renderer is selected
  and mounted, but scroll-region layout callbacks are no-ops.
- `resolvedEnabledModelsPersistTarget` helper: removed with the scoped-models
  selector gap.
- Anthropic subscription-auth warning: helper removed with the auth gap.

## Discrepancies

- `toLocaleString()` token counts are rendered with `en-US` grouping. The
  TypeScript output depends on the host locale; this port pins `en-US`.
- Tool elapsed timers use `Instant::now()` when the event carries an epoch
  timestamp, because `Instant` has no epoch mapping. Elapsed values are
  therefore relative to event receipt rather than the reported start.
- `MessageEnd` updates the component created at `MessageStart` in place; the
  source moves the component from the transcript tail into the chat container.
  Rendering is equivalent in regular mode.

## Differential

Not implemented for the whole mode: driving the real TypeScript
`InteractiveMode` requires a TTY/faux runtime harness that this slice did not
build. Deterministic helpers are covered by unit tests; the interactive
behavior tests drive the real Rust mode in a capture terminal with the faux
provider. This is recorded as a coverage gap in `coverage.md`.
