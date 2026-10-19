# session-picker traceability

Source: `packages/coding-agent/src/cli/session-picker.ts` (96 lines, pinned revision in
`docs/porting/source-lock.json`). Rust: `crates/pi-coding-agent/src/cli/session_picker.rs`.

Status: **done**. Both the data path (`loadSessionsWithControlNames`) and
`selectSession` are ported. The picker is driven by the real binary in
`tests/main_interactive.rs`.

## Exported symbols

| TS symbol | Rust | Tests |
| --- | --- | --- |
| `selectSession` | `select_session` (+ `SelectSessionOptions`, `SelectSessionOutcome`) | `resume_flag_opens_the_session_picker_and_cancels_with_escape`, `resume_flag_selects_a_session_and_starts_interactive_mode`; component behaviors in `tests/interactive_components_c.rs` |
| private `loadSessionsWithControlNames` | `load_sessions_with_control_names` | 6 tests in `tests/session_picker.rs` |

## Branches

| # | Branch | Test |
| --- | --- | --- |
| 1 | `archived ? isArchived : !isArchived` filter | `filters_by_archive_flag_and_forwards_progress`, `archived_sessions_are_ordered_only_by_modification_time` |
| 2 | `onProgress` forwarded to the loader | `filters_by_archive_flag_and_forwards_progress` |
| 3 | no `controlDbPath` → filtered list in loader order | `without_a_control_db_the_loader_order_is_kept` |
| 4 | `listNamedSessions` names override file names | `control_db_names_win_and_order_named_sessions_first` |
| 5 | non-archived: named sessions first | `control_db_names_win_and_order_named_sessions_first` |
| 6 | otherwise `b.modified - a.modified` (descending) | `an_empty_control_db_still_orders_by_modification_time`, `archived_sessions_are_ordered_only_by_modification_time` |
| 7 | loader rejection propagates | `loader_errors_propagate` |
| 8 | control-db read failure propagates | covered by `list_named_sessions`'s own tests; the picker maps the error to `String` |

## `selectSession` mapping

`select_session` builds `SessionSelectorComponent` with
`load_sessions_with_control_names` wrappers for the current/all/archived loaders,
`showRenameHint: false`, an installed `KeybindingsManager`, the control DB path,
and a selection validator. `create_startup_tui`/`start_startup_tui` provide the
TUI; the poll loop calls `process_events`/`flush_pending`, takes the component
out of its host while `run_pending_action` awaits, and maps select/cancel/exit to
`SelectSessionOutcome`. Validation failures re-render the header error and keep
the selector open (`SessionSelectorComponent::show_error`).

## Deviations

- **Loader futures are local** — the TUI runs on a single-threaded `LocalSet`, so
  `SessionLoader::load` returns a non-`Send` future and reuses
  `core::session_manager::SessionListProgress` (no duplicate alias, no `Send`
  bound). The component's `SessionListLoader` has the same shape.
- **`isArchived` truthiness** — `Option<bool>::unwrap_or(false)` matches the TypeScript
  truthy check.

## Proof

`cargo test -p pi-coding-agent --test session_picker` (6 tests) and
`cargo test -p pi-coding-agent --test main_interactive` (pty cancel and select cases).
