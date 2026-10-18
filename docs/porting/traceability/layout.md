# layout.ts traceability

Source: `packages/tui/src/layout.ts` (410 lines). Rust:
`crates/pi-tui/src/layout.rs`.

Tests: `crates/pi-tui/tests/layout.rs` (every listed TS layout test ported),
`crates/pi-tui/tests/differential_layout.rs` (14 scenarios comparing rendered
lines, box rects/clips, scroll state, and render-request counts),
`crates/pi-tui/tests/tab_width.rs` (tab overlay row accounting).

## Exported symbols

| TypeScript export | Rust item | Tests |
|---|---|---|
| `LayoutRect` | `LayoutRect` | layout/differential |
| `LayoutBox` | `LayoutBox` | layout/differential |
| `LayoutFrame` | `LayoutFrame` | layout/differential |
| `ScrollbarGeometry` | `ScrollbarGeometry` | scrollbar tests |
| `getScrollbarGeometry` | `get_scrollbar_geometry` | scrollbar tests, differential |
| `renderLayoutFrame` | `render_layout_frame` | all |
| `getScrollViewBox` | `get_scroll_view_box` | not exercised by listed tests |
| `getScrollViewsAt` | `get_scroll_views_at` | not exercised by listed tests |

## Branches

| TypeScript branch | Rust branch | Tests |
|---|---|---|
| render cache keyed by component + floored width | `LayoutContext::render_cached` | all |
| leaf allocation, cursor-line `lineOffset` | `line_offset_for_cursor` | layout/differential |
| scroll child layout at previous scroll top, then translate | `layout_scroll_node` | follow-end/nested tests |
| primary scroll view selection (`primary || first`) | `LayoutContext::primary_scroll_view` | differential |
| vstack intrinsic heights, basis override, gap total | `layout_vstack` | grow/shrink/nested-min tests |
| hstack intrinsic widths, widths allocation, zero-width child | `layout_hstack` | compose/zero-width tests |
| hstack align stretch/start/center/end | `layout_hstack` child offsets | differential center/end cases |
| painting: clip rows, OSC-133 prefix strip, kitty row crop, fast path, composite | `paint_box` | kitty crop test, differential |
| scroll image crop above the viewport | `paint_box` scroll branch | kitty crop test |
| scrollbar geometry (thumb height/offset, column/clip guard) | `get_scrollbar_geometry` | proportional scrollbar test |
| scrollbar cell styling with ANSI/grapheme preservation | `style_scrollbar_cell` | proportional scrollbar test |
| scroll view hit testing by clip/rect/depth | `get_scroll_views_at` | gap: no listed test |
| scroll view box lookup | `get_scroll_view_box` | gap: no listed test |

## Behavioral notes

- `LayoutBox.parent` is intentionally omitted: no consumer in this slice needs
  the back-reference, and Rust cannot build the source's cyclic tree without
  `Rc<RefCell>`. Boxes are addressed by child indices in tests.
- The transient scrollbar `setTimeout` becomes a deadline polled through
  `ScrollState::poll(now)`; the ported test drives `poll(now + 30ms)`.
- Scrollbar thumb math reproduces `Math.round` with `floor(x + 0.5)`.

## Gaps / exclusions

- `paints only clipped rows from very large scroll content`: the source test
  allocates a sparse 1e9-element JS array. Rust `Vec<String>` cannot represent
  that lazily, so the port uses 10k lines and asserts the same visible rows.
  Recorded deviation, not a behavior difference in painting.
- `getScrollViewBox` / `getScrollViewsAt` are implemented but the listed tests do
  not cover them; no differential scenario uses them.
- JavaScript callers passing non-finite geometry are excluded by the typed API.

## Discrepancies

None found.
