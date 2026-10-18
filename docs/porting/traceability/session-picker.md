# session-picker traceability

Source: `packages/coding-agent/src/cli/session-picker.ts` (96 lines, pinned revision in
`docs/porting/source-lock.json`). Rust: `crates/pi-coding-agent/src/cli/session_picker.rs`.

Status: partial. The data path (`loadSessionsWithControlNames`) is ported and tested.
`selectSession` is blocked on `SessionSelectorComponent`
(`modes/interactive/components/session-selector.ts`) and `cli/startup-ui.ts`.

## Exported symbols

| TS symbol | Rust | Tests |
| --- | --- | --- |
| `selectSession` | not ported (blocked) | — |
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

## Blocked part

`selectSession` constructs `SessionSelectorComponent` with the two loaders, a selection
validator, cancel/exit callbacks, render requests, and options
(`showRenameHint: false`, keybindings, `controlDbPath`, archived loader). It also needs
`createStartupTui`/`startStartupTui` from `cli/startup-ui.ts` (blocked on
`FirstTimeSetupComponent` and `ExtensionInputComponent`). When those land, `selectSession`
is a thin adapter over `load_sessions_with_control_names`.

## Deviations

- **Progress callback type** — the shared `SessionListProgress` alias is
  `&mut dyn FnMut(usize, usize)`, which is not `Send`; the picker's loader futures are
  `Send`, so `session_picker::SessionListProgress` adds `+ Send`. Callers pass a `Send`
  closure (the TUI progress renderer is).
- **`isArchived` truthiness** — `Option<bool>::unwrap_or(false)` matches the TypeScript
  truthy check.

## Proof

`cargo test -p pi-coding-agent --test session_picker` (6 tests).
