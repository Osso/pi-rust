# components/scroll-view.ts traceability

Source: `packages/tui/src/components/scroll-view.ts` (216 lines). Rust:
`crates/pi-tui/src/components/scroll_view.rs`.

Tests: `crates/pi-tui/tests/layout.rs` (follow-end, scrollbar geometry,
runtime reservation, nested geometry),
`crates/pi-tui/tests/differential_layout.rs` (scroll state + rendered frames).

## Exported symbols

| TypeScript export | Rust item | Tests |
|---|---|---|
| `ScrollViewScrollbar` | `ScrollViewScrollbar` | scrollbar tests |
| `ScrollViewOptions` | `ScrollViewOptions` | scrollbar tests |
| `ScrollViewScrollToOptions` | `ScrollViewScrollToOptions` | differential `scrollTo` |
| `class ScrollView` | `ScrollView` | all |
| getters `scrollTop`/`isFollowingEnd`/`viewportHeight`/`scrollbar`/`isScrollbarVisible` | same methods on `ScrollView`/`ScrollState` | follow-end/scrollbar tests |
| `setScrollbar` | `set_scrollbar` | runtime-reservation test |
| `getContentWidth` | `get_content_width` | always-scrollbar test |
| `setScrollbarActive` | `set_scrollbar_active` | gap: no listed test |
| `scrollTo` / `scrollBy` / `scrollToStart` / `scrollToEnd` | same | follow-end test, differential |
| `updateLayout` | `update_layout` | layout tests |
| `addChild`/`removeChild`/`clear` (throw) | not exposed | typed core exclusion |
| `render` | `Component::render` | scrollbar tests |
| `[LAYOUT_NODE]` | `Component::layout_node` | layout tests |

## Branches

| TypeScript branch | Rust branch | Tests |
|---|---|---|
| unsupported axis throws | `ScrollViewOptions` has no axis (typed exclusion) | n/a |
| scrollbar visibility: always/auto/hidden | `is_scrollbar_visible` | scrollbar tests |
| `getContentWidth` reserves a column for always | `get_content_width` | always test |
| `setScrollbar` hides transient or marks activity | `set_scrollbar` | runtime-reservation test |
| mark activity (auto + overflowing, deadline reset, active guard) | `mark_scrollbar_activity` | proportional test |
| transient hide timer | `poll(now)` + deadline | proportional test |
| `scrollTo` clamp/suppression/follow transitions | `scroll_to` | differential |
| `scrollBy` follow start, clamp, unused delta, follow update | `scroll_by` | follow-end test, differential |
| `scrollToStart`/`scrollToEnd` follow state | same | differential |
| `updateLayout` follow-end clamp, suppression reset, hide when fitting | `update_layout` | layout/differential |
| render pads when a scrollbar column is reserved | `Component::render` | always test |
| child mutation throws | not exposed | typed core exclusion |

## Behavioral notes

- All mutable scroll/scrollbar state lives in `ScrollState`, shared by the
  component and the layout node. `layout.rs` therefore drives the same state
  without holding a `ScrollView` handle.
- The transient-scrollbar `setTimeout` is a driver-owned deadline:
  `poll(now)` hides it. The ported test polls 30 ms after a 10 ms delay.
- The default scrollbar style is `\x1b[100m{text}\x1b[49m`, matching the source.

## Gaps / exclusions

- `setScrollbarActive` is implemented but not covered by the listed tests.
- `ScrollView` extends `Container` in the source with exactly one child; Rust
  exposes that child through `as_container` and omits the throwing mutation
  methods (`addChild`/`removeChild`/`clear`), which cannot be expressed without
  runtime errors in the typed API.

## Discrepancies

None found.
