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

## Q13 common-pin scheduling/container port (2026-10-22)

Frozen constructor :746–921 starts current load immediately. Native constructor now starts/polls the loader's synchronous prefix, retains its promise, and updates loading/progress live. Source :974–1036 uses independent scope tasks, caches before scope/sequence guards and suppresses stale progress/rendering. Native poll driver implements those transitions without holding the selector mutex across awaits; interactive mode polls rather than awaits full loads/actions. `load_current_sessions` only drains the constructor's existing task, not a second request.

`differential_session_selector` compares all render lines, inherited child frames, selected paths and full load/render/select/rename events after constructor and every operation. Held promises exercise progress, current/all/archived overlap, ignored stale progress, out-of-scope completion, cache re-entry, rename followed by held refresh, explicit error and initial load rejection. RED `session-selector-red.log` → GREEN `session-selector-green-attempt1.log` in `/tmp/pi-rust-repin-lossless-interactive/`.

Progress values are f64 (not usize); shared JS number formatting preserves fractions, negative values, NaN and Infinity. `session-selector-numeric-red.log` → `session-selector-numeric-green2.log` adds those exact header frames; native count-based loader adapters widen only at the boundary.

Actual Container with typed shared header/list/input children replaces manual rendering; rename panel retains frozen Text padding and inherited invalidation/focus. The old local delete helper is removed; source :872 now calls the shared `delete_session_tree`, archive :864 calls `archive_persisted_session`, and failures are not silently ignored. Mutation proof is still in progress; coverage remains open. No historical async/progress gaps are approved or retained.

## Discrepancies

None found.

## Q18 native UTF-16 integration (2026-10-22)

Session names/paths, rename/status/error callbacks and rendered frames use JsString. Search tokens, phrases, whitespace normalization and regex input are unit-native. `lossless_interactive_selectors` covers raw name render/path selection and raw query units; authored, unexecuted while coding-agent does not compile. No primary diagnostics at interactive6.
