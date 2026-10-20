# `tui/src/components/editor.ts` traceability

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
