# `tui/src/components/editor.ts` traceability

Baseline raw-editor repair (2026-10-25): `raw-tui-oracle.ts` now selects the same approved editor-scroll-width delta with HEAD/hash verification for both editor capture operations. Narrow-scroll snapshots compare exact raw UTF-16 frames; no frozen bytes or assertions were changed.

## Approved bottom-indicator delta (2026-10-24)

User approved matching bottom truncation to the existing top border. Standalone TS fix `88751b9de03a564e01edf00423f9bf1138b77a02`, based directly on common pin4f9e6f8a4, changes only editor.ts:883-893. [Delta lock](../delta-lock.json) records checkout and SHA256 `47bca9a925fbcf4e4f7f4ce1c14c33eaeb3d45e3c262d2e49174493824f3f976`; common-pin bytes remain untouched. `editor-oracle.mts` exclusively selects the approved editor module through the hash/HEAD-checked delta selector, never an old frozen editor substitute.

Behavioral REDs: `editor-bottom-frozen-red.log` observes actual frozen TS bottom width13 in a width3 frame; `editor-bottom-native-red.log` observes native untruncated colored bytes. After TS delta selection but before native change, `editor-bottom-delta-native-red.log` proves TS is bounded while native still diverges. Native `create_scroll_border` now applies the identical truncation branch to either direction.

`approved_bottom_indicator_fits_narrow_colored_frames_like_typescript` covers12 exact-frame cases:20/120 lines, widths3/7/10/12/13/20, real ANSI border colors and multi-digit counts. It asserts every source row's width and compares all native rendered bytes/text/cursor/callback state. Both legacy bug-preservation expectations now assert delta bytes; their width loops include the bottom line, not an excluded exception. Differential2/2 and rendering19/19 GREEN (`editor-bottom-differential-final.log`, `editor-bottom-rendering-green.log`). The reference shim is typed ESM with source-declared Editor/TUI/VirtualTerminal/AutocompleteProvider interfaces and the delta selector's explicit declaration, without any suppressions. Strict oracle+delta-module tsgo and Biome PASS. The standalone TS delta's complete npm check passes1243 files, dependency/import/install-lock checks, tsgo and browser bundle smoke; its checkout stays clean (`editor-bottom-ts-repo-check.log`).

Scoped pi-tui clippy exits0 with0 warnings/errors (`editor-bottom-clippy-final.{jsonl,log}`); workspace/all-targets JSON0 errors/0 warnings (`editor-bottom-workspace.{jsonl,log}`); fmt PASS. Manual changed-line Rust/TS readability audit retains the existing source render method and only mirrors its top-border branch. Inventory PASS686 rows/186679 lines with statuses399/19/88/180 (`editor-bottom-inventory.log`); no locked count/status changes. Parentf9167bc9 is merged. This is the approved width correction, not a new whole-TUI parity claim.

## Q13 AbortSignal request ordering (2026-10-22)

Frozen editor.ts:2478–2483 serializes provider tasks by awaiting the previous task before creating the next controller. Cancellation at :2601–2608 aborts that task with the default DOMException; it does not permit overlapping providers. Native `begin_autocomplete_request` now waits for `poll_autocomplete` to drain cancelled tasks before starting another provider.

`editor_abort` uses a real held provider future and frozen TS oracle. RED records native second-provider entry before the first return. GREEN asserts all four event/signal snapshots: AbortError DOMException/code 20, synchronous abort listener, first task return before second task entry, fresh non-aborted second signal, and completed controller detached from later cancellation. `editor_autocomplete` 20/20 GREEN. Logs: `/tmp/pi-rust-repin-lossless-interactive/editor-abort-{red,green}.log`, `editor-autocomplete-green.log`. No carrier or reason conversion added.

## Q2 frozen revalidation (supersedes historical pin claims below)

Frozen `4f9e6f8a4` editor.ts (2630 lines). Editor/EditorTheme/EditorOptions/TextChunk/wordWrapLine remain mapped below; whole editor targets and direct frozen scenario corpus are re-run. Oracle instantiates frozen tui.ts/TUI, not absent TuiMainScreen. Removed historyPrevious/historyNext handling and its old-only test. Top scroll indicator uses frozen truncateToWidth reset bytes. The frozen untruncated bottom bug (editor.ts:883-889) is superseded only by the [approved bottom-indicator delta](#approved-bottom-indicator-delta-2026-10-24); its byte/width proofs retain the original discrepancy evidence. Existing frozen editor-wide-wrap.test.ts is authoritative; the historical note calling the wide-wrap fix post-pin is obsolete for the current common pin. Poll-driven animation/autocomplete and UTF8 byte cursor adaptation remain explicit driver/type boundaries. See [Q2 proof](../q2-tui-components-proof.md).


Rust: `crates/pi-tui/src/components/editor.rs`.

Tests:

- `crates/pi-tui/tests/editor_word_wrap.rs` (22)
- `crates/pi-tui/tests/editor_editing.rs` (80)
- `crates/pi-tui/tests/editor_history.rs` (24)
- `crates/pi-tui/tests/editor_rendering.rs` (19)
- `crates/pi-tui/tests/editor_sticky.rs` (19)
- `crates/pi-tui/tests/editor_paste.rs` (22)
- `crates/pi-tui/tests/editor_autocomplete.rs` (20)
- `crates/pi-tui/tests/editor_working.rs` (3)
- `crates/pi-tui/tests/differential_editor.rs` (28 scripted scenarios against the real TS editor)

All 201 `it` cases in `packages/tui/test/editor.test.ts` and the single case in
`editor-history-keybindings.test.ts` have a Rust test. Some Rust test names are
shortened; behavior assertions follow the source cases. Cursor columns are
UTF-8 byte offsets (Rust mapping of UTF-16 code units); the differential oracle
converts TS columns to bytes.

## Exports

| Source export | Rust item | Tests |
|---|---|---|
| `TextChunk` | `TextChunk { text, start_index, end_index }` | word-wrap tests |
| `wordWrapLine` | `word_wrap_line(line, max_width, pre_segmented)` | all `editor_word_wrap` tests, differential `word-wrap` |
| `EditorTheme` | `EditorTheme { border_color, prompt_prefix, select_list }` | rendering, working, differential `prompt-prefix-padding` |
| `EditorOptions` | `EditorOptions { padding_x, autocomplete_max_visible }` | rendering, differential |
| `Editor` | `Editor` | all editor tests |

## Editor public API

| Source member | Rust item | Tests |
|---|---|---|
| `constructor(tui, theme, options)` | `Editor::new(Rc<dyn EditorHost>, EditorTheme, EditorOptions)` | every editor test |
| `focused` | `Focusable::focused/set_focused` | `focusable_trait_tracks_focus`, differential `focus-cursor-marker` |
| `borderColor` | `set_border_color` | `custom_theme_border_color_is_used` |
| `onSubmit` / `onChange` | `on_submit` / `on_change` | submit/undo/paste tests, differential callback arrays |
| `disableSubmit` | `disable_submit` | backslash workaround tests |
| `getPaddingX` / `setPaddingX` | `get_padding_x` / `set_padding_x` | padding render tests, differential `setPaddingX` |
| `getAutocompleteMaxVisible` / `setAutocompleteMaxVisible` | same names | differential `autocompleteMaxVisible` |
| `setAutocompleteProvider` | `set_autocomplete_provider(Rc<dyn AutocompleteProvider>)` | autocomplete tests |
| `setWorking` | `set_working` | working prompt tests, differential `working-static` |
| `setWorkingIndicator` | `set_working_indicator(Option<LoaderIndicatorOptions>) -> Result` | working prompt tests |
| `getPromptPosition` | `get_prompt_position` | prompt cell tests |
| `setScreenOrigin` / `clearScreenOrigin` | same names | prompt cell tests, differential |
| `addToHistory` / `getHistory` / `setHistory` | same names | history tests |
| `render` / `handleInput` / `invalidate` | `Component` impl | every render/input test |
| `getText` / `setText` / `getLines` / `getCursor` / `getExpandedText` | same names | accessor/normalization/paste tests |
| `insertTextAtCursor` | `insert_text_at_cursor` | insert-text tests |
| `isShowingAutocomplete` | `is_showing_autocomplete` | autocomplete tests |

## Internal behavior map

| Source function / branch | Rust item | Tests |
|---|---|---|
| `segmentWithMarkers`, `isPasteMarker` | `merge_paste_markers`, `is_paste_marker`, `segment_graphemes`, `word_segments` | paste-marker tests, word movement tests |
| `normalizeText` | `normalize_text` | setText CRLF/tab test, paste tests |
| `expandPasteMarkers` | `expand_paste_markers` | `expands_large_pasted_content_literally_in_get_expanded_text`, submit tests |
| `layoutText` / `buildVisualLineMap` / `findVisualLineAt` | same behavior in `layout_text`, `build_visual_line_map`, `find_visual_line_at` | wrapping, sticky, paste marker navigation |
| `render` prompt/scroll/cursor/padding branches | `Component::render` | all `editor_rendering`, differential renders |
| `renderHistorySearchLines` | `render_history_search_lines` | reverse-search tests |
| `handleHistorySearchInput` | `handle_history_search_input` | reverse-search tests |
| `navigateHistory` / drafts / cursor placement | `navigate_history` | history tests |
| `insertCharacter` undo coalescing | `insert_character` | undo tests |
| `handlePaste` (bracketed paste, CSI-u Ctrl decode, large markers, path spacing) | `handle_input` paste branch, `handle_paste`, `decode_csi_u_ctrl_letters` | paste tests |
| `handleBackspace` marker renumbering | `handle_backspace`, `renumber_paste_markers` | paste marker deletion/renumber tests |
| `handleForwardDelete` | `handle_forward_delete` | undo/delete tests, paste marker forward delete |
| `deleteToStartOfLine` / `deleteToEndOfLine` / `deleteWordBackwards` / `deleteWordForward` | same names in snake_case | kill-ring and undo tests |
| `moveToVisualLine`, `computeVerticalMoveColumn`, snapping | same names in snake_case | all `editor_sticky`, marker navigation tests |
| `moveCursor`, `pageScroll` | same names in snake_case | sticky/page tests |
| `moveWordBackwards` / `moveWordForwards` | same names in snake_case | Unicode/CJK word movement tests |
| `yank` / `yankPop` / insert/delete yanked text | same names in snake_case | kill-ring tests |
| `pushUndoSnapshot` / `undo` | same names | undo tests |
| `jumpToChar` + jump mode | `jump_to_char`, `JumpMode` | jump tests |
| `submitValue` | `submit_value` | submit/backslash/paste tests |
| `requestAutocomplete`, debounce patterns, force gating, stale request checks | `request_autocomplete`, `start_autocomplete_request`, `poll_autocomplete`, `build_trigger_pattern`, `build_debounce_pattern` | autocomplete tests, differential autocomplete scenarios |
| `applyAutocompleteSuggestions`, `getBestAutocompleteMatchIndex`, `createAutocompleteList` | same names in snake_case | autocomplete tests |
| `cancelAutocomplete` / request cancellation | `cancel_autocomplete`, `cancel_autocomplete_request`, cancelled-request queue | abort test |
| `getAutocompleteDebounceMs` | `get_autocomplete_debounce_ms` | `@`/`#`/custom trigger debounce tests |
| `setAutocompleteTriggerCharacters` | `set_autocomplete_trigger_characters` | provider reset test |
| `updateAutocomplete` after edits/cursor movement | `update_autocomplete`, `update_autocomplete_after_insert`, `update_autocomplete_after_edit` | autocomplete re-query/backspace tests |
| `restartWorkingAnimation`, `getWorkingPromptFrame`, prompt cell placement | `restart_working_animation`, `get_working_prompt_frame`, `update_prompt_cell_placement`, `poll_working` | working prompt tests |
| `createScrollBorder` | `create_scroll_border` | scroll indicator tests |

## Typed Rust exclusions

- `paddingX` and `autocompleteMaxVisible` are `usize`/`Option<usize>`; JavaScript
  non-finite/negative inputs (`Number.isFinite`, `Math.floor`) are not
  representable. Clamping (`3..=20`, padding to width) is preserved.
- `EditorTheme.borderColor`, `promptPrefix`, and select-list styles are
  `Rc<dyn Fn(&str) -> String>` rather than assignable JS properties.
- `AutocompleteProvider` is a trait (see `autocomplete.rs`) with an owned
  boxed future; the TS structural object is replaced by typed methods.
- Cursor columns are byte offsets, not UTF-16 code units. All editing and
  navigation operations use grapheme boundaries.

## Driver-owned behavior (deviations)

- `setInterval` working animation: no background timer. `poll_working(now)`
  advances frames at the deadline; fixed-cell writes and "no global render"
  behavior match.
- Async autocomplete requests: `handle_input` schedules a deferred request;
  `begin_autocomplete_request()` starts it and `poll_autocomplete().await`
  resolves it. Cancelled requests are drained by the next poll so providers
  observe `AbortSignal`-equivalent cancellation. This preserves the source's
  async ordering (snapshot taken when the request starts, not when input
  schedules it).
- `structuredClone` is replaced by `Clone`.

## Gaps / discrepancies

- Locale case folding: TS `toLocaleLowerCase()` vs Rust `to_lowercase()` in
  reverse history search. ASCII behavior matches; locale-specific folding
  (e.g. Turkish dotted I) can differ. Documented gap.
- Source bug preserved: forward-deleting a paste marker removes the marker text
  but leaves the paste registry entry; undo restores it through the snapshot.
- Source behavior preserved: backspace on a paste marker decrements
  `pasteCounter`, which can go negative only if the registry is already empty.
  Rust uses `i64` to preserve the value instead of trapping.
- `Intl.Segmenter` dictionary word differences are the approved global ICU4X
  deviation (see `word-navigation.md`).

## Wrapped cursor offset

`layout_lines` computes the relative cursor offset only when the chunk contains it, matching TS editor.ts:1285–1293. Narrow-width modal-example input/render differential exposed and now covers the previous Rust subtraction overflow. See [editor example proofs](extension-examples-editors.md).

## Overwide graphemes (post-pin fix)

User approved fixing both roots October 20, 2026. TS commit `01870b0b2`
(`packages/tui/test/editor-wide-wrap.test.ts`) supplies the fixed expectations;
pinned TS throws `RangeError`, so it cannot be an oracle for these cases.

| Behavior | Native proof (`editor_wide_wrap`) |
|---|---|
| Indivisible overwide emoji, CJK, ZWJ; consecutive/mixed text; exact contiguous offsets | `overwide_graphemes_overflow_alone_and_preserve_chunk_offsets` |
| Merged multi-grapheme visual segment still splits and terminates | `merged_visual_segments_still_split_into_graphemes_at_width_one` |
| Render, cursor chunk membership, vertical navigation in both directions | `narrow_rendering_and_visual_navigation_agree_on_overwide_chunks` |

Exact width-one chunk strings: `🚀` → [`🚀`], `a🚀b` → [`a`, `🚀`, `b`],
`🚀🚀` → [`🚀`, `🚀`], `界a` → [`界`, `a`], `👩‍🚀x` → [`👩‍🚀`, `x`].
At render width two, `a🚀b` yields content [`a `, `🚀`, `b `]. TS cursor
columns 4 → 1 → 0 → 1 → 4 map to Rust bytes 6 → 1 → 0 → 1 → 6.

Both direct wrap consumers were audited: `layoutText` / `layout_text` feeds
render and cursor placement; `buildVisualLineMap` / `build_visual_line_map`
feeds cursor lookup, visual navigation, paging, and history boundary checks.
They use chunk ranges, not an assumption that chunk widths fit. No consumer
change is needed. The editor files have no other width-splitting recursion or
loop with this zero-progress pattern; grapheme navigation always consumes a
segment. The separate utils long-word splitter consumes every grapheme too.

All three regressions run in child processes so the pre-fix Rust stack abort
is an assertion failure in the parent, not a harness abort. RED: 3 failed in
each language; GREEN: 3 passed each. Removing each progress guard kills all
three regressions (3 failed each). Logs and revision/diff proof ledger:
`/tmp/pi-editor-wide-wrap/ledger.md`. Existing pinned editor differentials
remain applicable only to non-defective inputs.
