# session-selector.ts

Frozen: `coding-agent/src/modes/interactive/components/session-selector.ts` at `4f9e6f8a4` (1073 lines).
Native: `crates/pi-coding-agent/src/modes/interactive/components/session_selector.rs`.

## Symbol and branch map

| Frozen symbols / branches | Native representation | Proof |
|---|---|---|
| shortenPath, formatSessionDate, canonicalizePath (:30–58) | unit-native paths; f64 Date values; JS age bucket number formatting | existing nine sequences; raw selector tests; list corpus |
| SessionSelectorHeader setters/render/loading/progress/hints/status (:60–200) | typed header child, f64 progress, deadline-driven status timeout | lifecycle/numeric/full-list frame+event differential, widths0/1/8/30/90; status timeout proof pending |
| buildSessionTree/compareSessionTreeNodes/flattenSessionTree (:221–297) | indexed last-node Map identity, shared JS sort, literal connectors/activity propagation | fourteen-row corpus with parent/grandchild, duplicate paths and two pages |
| SessionList constructor/accessors/filter/sort/name/session/current-delete guards/render/input (:302–670) | typed shared list/input; Component input and focus delegates; native UTF-16 normalization/search | six list sequences include empty name, lone surrogate/control units, paging, sort/search/path/delete confirmation; direct child input/focus RED→GREEN |
| SessionSelectorComponent constructor/layout/focus/handleInput/getSessionList/showError (:677–844,1065–1072) | actual inherited Container; typed retained header/list/rename handles; getter returns actual list handle | complete child/component frames and selected paths; direct list input/focus, padded rename panel, errors and callback state sequences |
| archive/delete callbacks (:846–917) | shared archive_persisted_session/delete_session_tree; no local duplicate deletion or ignored metadata errors | five real-file/control-DB differential cases: recursive delete, partial delete, archive/compressed relocation, missing DB, already archived |
| loadCurrentSessions/loadScope/toggleScope (:924–926,973–1036,1047–1063) | constructor-started independent futures; live progress; cache-before-scope/sequence guards; bounded driver polling | held current/all/archived promises, stale progress, out-of-scope completion, cache re-entry, initial rejection; double refresh newest-first |
| enterRenameMode/exitRenameMode/confirmRename (:929–971) | actual panel rebuild; trim/undefined name, refresh before finally exit | current archived rename with held refresh, existing rename behavior test; rejected rename proof pending |
| toggleSortMode/toggleNameFilter/refreshSessionsAfterMutation (:1038–1045) | literal state cycles and header/list updates | exact frames and render events in lifecycle/list/mutation sequences |

The Rust poll driver starts each loader's synchronous prefix at invocation, then processes promise continuations without holding the selector mutex across awaits. `load_current_sessions` drains the already-started constructor task, never issues a second initial request. Interactive mode no longer awaits full loads/actions. CLI driver integration remains open pending its blocked-loader proof. No historical async/progress gap is retained or approved.

`getSessionList` built-in consumer audit: frozen `cli/session-picker.ts:70` focuses the returned live list. Native returns the retained typed handle; the list implements Component input/focus, and selector polling drains its events. Direct-child regression RED demonstrated both formerly missing trait delegates. Owned-copy approval does not erase this required live-child behavior.

## Current proof ledger

All logs under `/tmp/pi-rust-repin-lossless-interactive/`.

- `session-selector-red.log` → `session-selector-green-attempt1.log`: constructor/progress/concurrent scope and rename frames/events.
- `session-selector-numeric-red.log` → `session-selector-numeric-green2.log`: fractional/negative/NaN/Infinity header progress, shared JS number formatting.
- `session-selector-list-red2.log` → `session-selector-list-green-attempt1.log`: empty-message color applied after truncation (:455), empty-name truthiness (:477), default final-row ellipsis `...` (:533), last-node Map identity (:225–242). Six full-list scenarios; fourteen rows exceed page size.
- `session-selector-child-input-red.log`, `session-selector-child-focus-red.log` → `session-selector-child-green2.log`: direct list input/focus, plus double-refresh ordering. Five lifecycle/numeric/child scenarios and six full-list scenarios GREEN.
- `session-selector-mutations-green.log`: five isolated source/native real file+SQLite cases. Private Python trash fixture; no live data. `session-selector-existing.log`: seven behavior tests GREEN. `session-selector-old-nine-green.log`: nine existing frozen differential cases GREEN, moved to a separately filterable test without changing assertions or dropping cases.

## DISCREPANCY — stale cache publication

Frozen :991–1005 publishes a completed all-scope load to `allSessions` before checking its sequence. An older completion is suppressed on screen, but overwrites the cache and becomes visible after cycling away/back. Parent classified/preserved this literally on 2026-10-22. Double-Enter rename refreshes completed newest-first exercise the exact boundary; native matches. No corrective delta is introduced.

No other source discrepancy identified. Human text and callbacks remain JsString; OS paths use the existing shared Node boundaries. Progress is f64. No lossy projection or alternate delete/archive implementation remains in this module.
