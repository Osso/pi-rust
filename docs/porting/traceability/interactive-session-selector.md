# components/session-selector.ts

Source: `coding-agent/src/modes/interactive/components/session-selector.ts`
Rust: `crates/pi-coding-agent/src/modes/interactive/components/session_selector.rs`

Tests: `crates/pi-coding-agent/tests/interactive_components_c.rs` (session_selector_*),
differential `sessionSelector` cases (9 scope/filter/sort/search/path sequences with exact
ANSI lines and selected paths).

| Symbol / branch | Rust item | Proof |
|---|---|---|
| `shortenPath` | `shorten_path` | path-toggle differential |
| `formatSessionDate` | `format_session_date` | age buckets in render |
| `canonicalizePath` | `canonicalize` (`utils::paths`) | current-session marker |
| `SessionSelectorHeader` (scope/sort/name, loading progress, hints, status) | `SessionSelectorHeader` | differential + status tests |
| header status auto-hide | `SessionSelectorHeader::poll` | status test |
| `buildSessionTree` / `compareSessionTreeNodes` / `flattenSessionTree` | same-named helpers | threaded differential |
| `SessionList` constructor and filtering | `SessionList::new`/`filter_sessions` | differential |
| `setSortMode`/`setNameFilter`/`setSessions` | same-named methods | differential |
| `startDeleteConfirmationForSelectedSession` | same-named method | delete test |
| `isCurrentSessionPath` | same-named method | current-delete test |
| `buildTreePrefix` | `build_tree_prefix` | threaded differential |
| `render` (search line, empty messages, rows, scroll indicator) | `Component::render` | differential |
| `handleInput` (confirm state, tab, sort, named, path, archive, delete, rename, non-invasive delete, navigation, confirm/cancel, search) | `SessionList::handle_input` | differential + behavioral |
| `deleteSessionFile` (`trash` then `unlink`, error hint) | `delete_session_file` | delete test |
| `SessionSelectorComponent` scope cache/toggle | `toggle_scope` | scope differential |
| `loadScope` (initial/refresh/toggle, seq guards, progress, error) | `load_scope` | scope/error tests |
| `toggleSortMode` / `toggleNameFilter` | same-named methods | differential |
| `enterRenameMode` / `exitRenameMode` / `confirmRename` | same-named methods | rename test |
| `onDeleteSession` (metadata removal, list update, refresh) | `delete_session` | delete test |
| `onArchiveSession` (control DB required, already-archived, archive+refresh) | `archive_selected_session` | archive test |
| `loadCurrentSessions` from constructor | `load_current_sessions` (async) | differential |
| `getSessionList` | `session_list` | differential |
| `showError` | `show_error` (error status, 4 s auto-hide, render) | `resume_flag_opens_the_session_picker_and_cancels_with_escape` error path; header status tests |

## Gaps

- Async boundaries: the constructor does not start loading; the caller awaits
  `load_current_sessions()`. Loader futures are non-`Send` (single-threaded TUI). `handle_input` queues async work (delete/archive/scope load/rename
  submit) and the caller awaits `run_pending_action()`. State ordering matches the source
  (loading set before the await, results applied after, sequence guards preserved).
- Loader progress is recorded in a shared cell and applied to the header after the loader
  resolves; intermediate progress renders require the caller to drive loading separately.
- `delete_session_file` maps a missing `trash` binary to Node's `spawnSync trash ENOENT`
  message only in the unlink-failure hint.

## Discrepancies

None found.

## Q18 native UTF-16 integration (2026-10-22)

Session names/paths, rename/status/error callbacks and rendered frames use JsString. Search tokens, phrases, whitespace normalization and regex input are unit-native. `lossless_interactive_selectors` covers raw name render/path selection and raw query units; authored, unexecuted while coding-agent does not compile. No primary diagnostics at interactive6.
