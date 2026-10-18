# alt-screen-search.ts traceability

Source: `packages/tui/src/alt-screen-search.ts` (157 lines). Rust:
`crates/pi-tui/src/alt_screen_search.rs`.

## Exported symbols

| TypeScript | Rust | Tests |
| --- | --- | --- |
| `AltScreenSearchSegment` | `AltScreenSearchSegment` | `alt_screen_search`, differential |
| `AltScreenSearchMatch` | `AltScreenSearchMatch` | `alt_screen_search`, differential |
| `findAltScreenSearchMatches` | `find_alt_screen_search_matches` | `alt_screen_search`, differential |
| `getAltScreenSearchMatchKey` | `get_alt_screen_search_match_key` | `alt_screen_search`, differential |
| `AltScreenSearchComponent` | `AltScreenSearchComponent` | `alt_screen_search`, differential |

## Branch coverage

| Source branch | Rust item | Tests |
| --- | --- | --- |
| empty/whitespace-only normalized query returns no matches | `find_alt_screen_search_matches` early return | `empty_or_whitespace_queries_return_no_matches` |
| query whitespace collapse and trim (`\s+` → space) | `normalize_query` | `collapses_whitespace_in_queries_and_matches_case_insensitively`, differential |
| case-insensitive matching (`giu`) | `RegexBuilder::case_insensitive` | `collapses_whitespace...`, differential `CAFÉ`/`MiXeD` |
| line-boundary and leading/trailing whitespace separators (`pendingSeparator`) | `build_search_corpus` | `treats_line_boundaries_and_leading_whitespace_as_single_spaces` |
| terminal escape stripping before segmentation | `strip_terminal_sequences` per line | `strips_terminal_sequences_before_mapping_columns` |
| grapheme segmentation and cell-column mapping | `graphemes(true)`, `visible_width` | `maps_wide_graphemes_to_cell_columns` |
| segment merging when spans are adjacent on the same row | `find_alt_screen_search_matches` merge loop | `maps_wide_graphemes_to_cell_columns`, `searches_normalized...` |
| unmapped separator spans split segments | merge loop `None` skip | `collapses_whitespace...`, differential |
| multiple matches in corpus order | `find_iter` | `reports_every_match_in_order`, differential |
| match key formatting and empty match | `get_alt_screen_search_match_key` | `builds_stable_match_keys`, differential |
| component status: empty query / no matches / `index+1/count` | `AltScreenSearchComponent::status` | `renders_the_title_status_and_input_line` |
| title gap minimum of one column | `gap_width.max(1)` | `renders_the_title_status_and_input_line` |
| title truncation and padding to safe width | `truncate_to_width_with(..., "", false)` | `truncates_the_title_to_the_safe_width` |
| input line rendered at safe width | `Input::render` | `renders_the_title_status_and_input_line` |
| query callback only on value change | `handle_input` comparison | `forwards_focus...`, differential |
| focus forwarding to input | `Focusable::set_focused` | `forwards_focus...` |
| invalidate forwarding | `Component::invalidate` | `forwards_focus...` (render after focus) |

## Differential

`tests/differential_alt_screen_search.rs` drives `oracle/tui-alt-screen-oracle.ts`
with 15 transcript/query cases (including ANSI, wide graphemes, combining marks,
OSC 133, regex metacharacters, accented case folding) and a 15-step search
component script. It compares match segments, match keys, rendered lines, and
reported query changes exactly.

## Deviations

- Corpus indices are Rust byte offsets; one source-span entry is stored per byte
  of each grapheme. Source spans are cell columns, so resulting segments match
  the TypeScript UTF-16 scan.
- `\s` additionally treats `U+FEFF` as whitespace to match JavaScript regex
  semantics.
- The component exposes `set_result`/`new`/`render`/`handle_input` directly;
  TypeScript's getter/setter `focused` maps to the `Focusable` trait.
