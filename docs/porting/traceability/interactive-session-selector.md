# session-selector.ts

Frozen: `coding-agent/src/modes/interactive/components/session-selector.ts` at `4f9e6f8a4` (1073 lines).
Native: `crates/pi-coding-agent-ui/src/modes/interactive/components/session_selector.rs`.

## Symbol and branch map

| Frozen symbols / branches | Native representation | Proof |
|---|---|---|
| shortenPath, formatSessionDate, canonicalizePath (:31–59) | unit-native paths; f64 Date values; JS age bucket number formatting | existing nine sequences; raw selector tests; list corpus |
| SessionSelectorHeader setters/render/loading/progress/hints/status (:60–200) | typed header child, f64 progress, deadline-driven status timeout | lifecycle/numeric/full-list frame+event differential, widths0/1/8/30/90; real status timeout replacement/clear proof |
| buildSessionTree/compareSessionTreeNodes/flattenSessionTree (:222–297) | indexed last-node Map identity, shared JS sort, literal connectors/activity propagation | fourteen-row corpus with parent/grandchild, duplicate paths and two pages |
| SessionList constructor/accessors/filter/sort/name/session/current-delete guards/render/input (:303–667) | typed shared list/input; Component input and focus delegates; native UTF-16 normalization/search | six list sequences include empty name, lone surrogate/control units, paging, sort/search/path/delete confirmation; direct child input/focus RED→GREEN |
| SessionSelectorComponent constructor/layout/focus/handleInput/getSessionList/showError (:675–893,1065–1072) | actual inherited Container; typed retained header/list/rename handles; getter returns actual list handle | complete child/component frames and selected paths; direct list input/focus, padded rename panel, errors and callback state sequences |
| archive/delete callbacks (:834–889) | shared archive_persisted_session/delete_session_tree; no local duplicate deletion or ignored metadata errors | five real-file/control-DB differential cases: recursive delete, partial delete, archive/compressed relocation, missing DB, already archived |
| loadCurrentSessions/loadScope/toggleScope (:895–897,957–1020,1041–1063) | constructor-started independent futures; live progress; cache-before-scope/sequence guards; bounded driver polling | held current/all/archived promises, stale progress, out-of-scope completion, cache re-entry, initial rejection; double refresh newest-first |
| enterRenameMode/exitRenameMode/confirmRename (:899–955) | actual panel rebuild; trim/undefined name, refresh before finally exit | archived rename with held refresh, trimmed callback argument, rejected rename finally-exit/rejection propagation, existing rename behavior test |
| toggleSortMode/toggleNameFilter/refreshSessionsAfterMutation (:1022–1039) | literal state cycles and header/list updates | exact frames and render events in lifecycle/list/mutation sequences |

The Rust poll driver starts each loader's synchronous prefix at invocation, then processes promise continuations without holding the selector mutex across awaits. `load_current_sessions` drains the already-started constructor task, never issues a second initial request. Interactive mode no longer awaits full loads/actions. CLI picker likewise keeps the selector present, polls once per turn and focuses the live list handle (:70 in frozen session-picker.ts). `session_picker_concurrency` RED timed out before showing UI; GREEN cancels through Escape while both current/all loader futures remain permanently pending, asserts two started loads, visible progress and terminal start/stop. No live terminal or provider is used. No historical async/progress gap is retained or approved.

`getSessionList` built-in consumer audit: frozen `cli/session-picker.ts:70` focuses the returned live list. Native returns the retained typed handle; the list implements Component input/focus, and selector polling drains its events. Direct-child regression RED demonstrated both formerly missing trait delegates. Owned-copy approval does not erase this required live-child behavior.

## Current proof ledger

All logs under `/tmp/pi-rust-repin-lossless-interactive/`.

- `session-selector-red.log` → `session-selector-green-attempt1.log`: constructor/progress/concurrent scope and rename frames/events.
- `session-selector-numeric-red.log` → `session-selector-numeric-green2.log`: fractional/negative/NaN/Infinity header progress, shared JS number formatting.
- `session-selector-list-red2.log` → `session-selector-list-green-attempt1.log`: empty-message color applied after truncation (:455), empty-name truthiness (:477), default final-row ellipsis `...` (:533), last-node Map identity (:225–242). Six full-list scenarios; fourteen rows exceed page size.
- `session-selector-child-input-red.log`, `session-selector-child-focus-red.log` → `session-selector-child-green2.log`: direct list input/focus, plus double-refresh ordering. Five lifecycle/numeric/child scenarios and six full-list scenarios GREEN.
- `session-picker-concurrency-red.log` → `session-picker-concurrency-green.log`: separate bounded native process through actual select_session; no held UI borrow/await or transient missing component during loads/actions.
- `session-selector-mutations-green.log`: five isolated source/native real file+SQLite cases. Private Python trash fixture; no live data. `session-selector-existing.log`: seven behavior tests GREEN. `session-selector-old-nine-green.log`: nine existing frozen differential cases GREEN, moved to a separately filterable test without changing assertions or dropping cases.

`session-selector-full-green-attempt1.log`: both tests GREEN after all production changes; eleven lifecycle/numeric/concurrency/child/container/no-archive-loader/rejected-rename/status scenarios plus six full-list scenarios. Real status timers prove replacement fires once, timeout clears displayed text and requests render, and selecting a session cancels the next status timeout. Inherited add/remove/clear/invalidate retain private list behavior and rebuild the layout on rename/exit.

## OS-home boundary

Isolated `differential_session_selector` HOME probes went RED in `session-selector-home-red.log`, now GREEN in `homedir-consumers-green.log`. Frozen :31–36 calls os.homedir(), not HOME||USERPROFILE. Native uses the [shared Node authority](node-os.md): missing HOME on POSIX uses the effective-UID passwd entry. Present-empty HOME remains distinct and prefixes every nonempty displayed path with `~` (DISCREPANCY preserved literally). Existing non-environment proofs above remain valid.

## DISCREPANCY — empty-list navigation

Frozen Down/PageDown on an empty or pending list assigns selectedIndex=-1 (:638–647). `filterSessions` preserves it after sessions arrive (:410), so no selected row/Enter callback exists until Up/PageUp restores index0. Native formerly clamped0. `session-selector-empty-navigation-red.log` → `session-selector-full-green-attempt1.log` reproduces both keys and recovery; signed indexing now preserves the source behavior. Parent classified/preserved on 2026-10-22; no corrective delta.

## DISCREPANCY — stale cache publication

Frozen :991–1005 publishes a completed all-scope load to `allSessions` before checking its sequence. An older completion is suppressed on screen, but overwrites the cache and becomes visible after cycling away/back. Parent classified/preserved this literally on 2026-10-22. Double-Enter rename refreshes completed newest-first exercise the exact boundary; native matches. No corrective delta is introduced.

No other source discrepancy identified. Human text and callbacks remain JsString; OS paths use the existing shared Node boundaries. Progress is f64. No lossy projection or alternate delete/archive implementation remains in this module.

## Render-boundary homedir characterization

Characterization, no RED by design (main approval 2026-10-23): the old in-chain native panic and the replacement Result-to-Component-boundary failure have identical observable crash behavior. `interactive_crash::session_path_render_home_failure_restores_terminal_before_crash_report` loads real JSONL + control metadata, opens `/resume` in a PTY, then enables NSS EIO and toggles path display. It exercises SessionList → TUI render → installed interactive uncaughtCrash handler, asserting exit 1, native SystemError, cursor restoration and terminal restore before the crash banner. Baseline GREEN on production `1fe65e81` plus the characterization test: `/tmp/pi-rust-repin-lossless-interactive/session-selector-home-baseline-green.log`.

Frozen `tui.ts:1895–1901` increments renderGeneration and clears rendering in finally. Rust's panic hook runs before unwinding; this ordering differs internally but is unobservable in this terminal crash path: `interactive-mode.ts:5207–5223` uncaughtCrash only unregisters signals, kills children, calls ui.stop and reports/exits; `tui.ts:931–959` stop reads timers/focus/previousLines/hardwareCursorRow, not rendering/generation; `terminal.ts:421–465` stop handles keyboard/stdin/raw state and :524–525 shows cursor. The rendering/generation readers at `tui.ts:1630,1700,1750,1760` are render-region paths, never called by this crash shutdown. No guard, deferred dispatch, or fallible Component/Container API was added.

Carrier refactor: `shorten_path` → private `render_rows` now propagates NativeNodeError by Result; only SessionList Component::render calls shared `throw_native_uncaught`. The same PTY characterization remains GREEN (`session-selector-home-green.log`) after baseline test commit `e6cee584`. No TUI renderer/Component/Container change.
