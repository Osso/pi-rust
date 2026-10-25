# session-picker traceability

Frozen source: `packages/coding-agent/src/cli/session-picker.ts` at `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d` (all 90 lines audited).
Rust: `crates/pi-coding-agent/src/cli/session_picker.rs`.

| Symbol / branch | Rust | Proof |
|---|---|---|
| `selectSession`, validation/cancel/selection | `select_session`, `SelectSessionOutcome` | existing `main_interactive` PTY picker cases |
| `loadSessions`, archive filtering and progress | `load_sessions` | `filters_by_archive_flag_and_forwards_progress` |
| Names come from each loaded `SessionInfo.name`, no duplicate name lookup | `is_named` | `metadata_names_order_named_sessions_first_then_by_modification_time` |
| Blank names are unnamed (JS trim) | `is_named` | `blank_names_do_not_count_as_named` |
| Active named-first ordering, then modified-desc within group | `load_sessions` | metadata-name and unnamed-order tests |
| Archived sessions ignore named-first ordering | `load_sessions` | `archived_sessions_are_ordered_only_by_modification_time` |
| Loader rejection propagates | `load_sessions` | `loader_errors_propagate` |

`select_session` passes the control DB path to the selector for mutations, not
for overlaying names. `SessionManager::list*` supplies authoritative metadata names.
Loader futures remain local to the TUI `LocalSet`; no additional dependency.

## Q18 native UTF-16 integration (2026-10-22)

SessionLoader errors, selected path, control-db path and validation callbacks retain JsString. `lossless_interactive_selectors::picker_selection_and_validation_retain_unpaired_path_units` is authored, unexecuted. No primary diagnostics at interactive6.

## Common-pin resident filtering

`loadSessions` excludes resident ids and resident-directory paths before archive filtering/sorting. Added missing shared `is_resident_session` predicate at the same position. RED `/tmp/settings-models-picker-red.log` shows supervisor id/path entries leaked into the current list. `suite_session_1::session_picker::hides_resident_sessions_from_current_and_archived_lists` covers both ids, both resident directories and both archive modes.

Source `loadSessions` is private; no source rewriting or test-only export is introduced. Native concrete loader/progress/order/error tests cover it. `selectSession` startup TUI/keybindings, single-resolution selection, validation-error retry, cancel/exit, render/focus and stop branches map to the existing native/PTY picker fixtures; the frozen interactive no-whole-mode-differential approval applies. Module-load/microtask ordering oddities remain outside the parity bar.

## Native terminal-boundary proof

`startup_ui::session_picker_validation_retry_and_cancel_reach_the_terminal_boundary` runs real select_session with a scripted terminal in a fresh process: explicit dark theme prevents probe input consumption, and input waits for the rendered validation error before retry. It asserts the selected path, visible error text, terminal stopped state, and Escape/Ctrl+C cancellation. No production timing changes were made. Frozen `SessionList.handleInput` never invokes onExit; that keyboard path is unreachable (parent-confirmed 2026-10-25), not a missing native keyboard feature.

GREEN: df6a73a6e: /tmp/settings-models-picker-green.log (7 loaders);9ad0f9f28: /tmp/settings-models-picker-ui-green.log (visible validation retry and both cancel keys). Targeted warning-mode clippy at69513786b: `/tmp/settings-models-checkpoint-clippy.log`, exit0; existing warnings at control_command.rs:55 and sessions_command.rs:136 are unchanged conversions, none on changed lines. Changed Rust/TS manually audited for readability; no in-scope violations.
