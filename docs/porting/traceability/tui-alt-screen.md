# tui-alt-screen.ts traceability

Source: `packages/tui/src/tui-alt-screen.ts` (1291 lines). Rust:
`crates/pi-tui/src/tui_alt_screen.rs`. Search overlay:
`alt-screen-search.md`. Viewport helpers live in `crates/pi-tui/src/tui.rs`.

Tests: `tests/tui_alt_screen.rs` (36 ported cases) and
`tests/differential_tui_alt_screen.rs` (23 scripted scenarios against the real
TypeScript renderer).

## Exported symbols

| TypeScript | Rust | Tests |
| --- | --- | --- |
| `TuiAltScreenOptions` | `TuiAltScreenOptions` | ported tests, differential |
| `TuiAltScreen` | `TuiAltScreen` (`Rc<TuiAltScreen>`) | ported tests, differential |
| `TuiAltScreen.flash` | `flash(message, Option<Duration>)` | flash stacking |
| `TuiAltScreen.viewportTop` / `isFollowingOutput` | `viewport_top` / `is_following_output` | scroll tests, differential |
| `TuiAltScreen.setLayoutRoot` | `set_layout_root` | dock/overlay tests, differential |
| `VIEWPORT_TUI` / `ViewportTUI` / `isViewportTUI` | `ViewportTui`, `TuiExt::as_viewport_tui`, `is_viewport_tui` | differential `focus`/layout scenarios |
| `TuiBase.beforeTerminalStart` / `afterTerminalStop` | `RendererApi::before_terminal_start` / `after_terminal_stop` | start/stop tests, differential |

## Branch coverage

| Source branch | Rust item | Tests |
| --- | --- | --- |
| enter alt screen: capability capture, iTerm2 capability swap, state reset, multiplexer vs all-motion mouse sequence | `before_terminal_start_impl` | multiplexer test, iTerm2 tests, differential |
| leave alt screen: close search, clear selection/drag/hover, kitty image deletion, autowrap restore | `before_terminal_stop_impl` | selection/focus tests, differential stop |
| after stop: preserve screen vs restore full document with line resets, clipping, cursor and autowrap | `after_terminal_stop_impl`; shared `render_stop_document` / `stop_document_output`; `terminal_stop_output` publishes the default stop snapshot with `stop_before_output` | full-document restore test, differential; coding-agent `interactive_crash::worker_restore_matches_normal_ui_stop_bytes` verifies exact real-terminal bytes and termios on another thread |
| implicit document/scroll view when no layout root | `ImplicitDocument`, `implicit_scroll_state` | scroll tests, differential |
| explicit layout root render and invalidation (root + overlays) | `render`, `invalidate` | dock, overlay invalidation tests |
| primary scroll view selection and fallbacks | `primary_scroll_view` | all scroll tests |
| scrollBy / scrollToTop / scrollToBottom | `scroll_by`, `scroll_to_top`, `scroll_to_bottom` | scroll tests, differential |
| prompt jumps over OSC 133 `A` markers in both directions | `scroll_to_prompt`, `is_prompt_start` | prompt-jump test, differential |
| search open/focus, close, query update anchor, next/previous/retain selection modes, reveal scroll | `open_search`, `close_search`, `update_search_query`, `navigate_search`, `refresh_search` | search tests, differential |
| search highlights: per-row segments, current style, scrollbar column clamp, ANSI-preserving styling | `apply_search_highlights`, `apply_search_text_highlight` | search tests, differential |
| mouse parsing: SGR wheel/button/release, X10 wheel, horizontal wheel rejection, non-mouse SGR consumption | `parse_sgr_fields`, `parse_wheel_event`, `parse_sgr_mouse_event`, `is_mouse_sequence` | wheel tests, horizontal-wheel test, differential |
| wheel routing: deepest scroll view first, overscroll contain, leftover chaining to primary | `route_wheel` | wheel routing/chaining tests, differential |
| scrollbar target/hover/drag, thumb offset rounding, release, selection cancel | `scrollbar_target_at`, `set_scrollbar_hover`, `handle_scrollbar_mouse_event` | scrollbar drag/column tests |
| right-click paste only on win32, unmodified secondary press, best-effort handler | `handle_right_click_paste` | right-click test, differential |
| selection points: scroll-view mapping, content-row clamp, terminal clamp | `get_scroll_selection_point`, `selection_point` | selection tests, differential |
| word/line selection granularity, double/triple click counting, drag extension, whitespace segments | `word_selection`, `line_selection`, `click_count`, `update_selection_focus` | word/line/whitespace tests, differential |
| selection bounds ordering, cross-scroll-view rejection, zero-width rejection | `selection_bounds` | focus-loss tests |
| selection auto-scroll edge direction and stop conditions | `update_selection_auto_scroll`, `auto_scroll_selection`, `stop_selection_auto_scroll` | auto-scroll test |
| grapheme-aware selection columns and inverse-video reapplication | `get_selection_columns`, `apply_selection_highlight`, `apply_selection` | CJK/emoji/combining test, selection test |
| OSC 52 clipboard copy, trim, flash | `copy_selection_to_clipboard` | selection tests |
| OSC 8 link click (no drag) and best-effort open | `handle_selection_mouse_event` URL branch | link test, differential |
| focus out/in: cancel press, clear hover/drag, cancel visible selection only when non-empty | `handle_viewport_input` focus branches | focus-loss tests, differential |
| keyboard viewport bindings, release suppression, Ctrl-modified passthrough | `handle_viewport_input` key branches | keyboard/custom-binding/Ctrl tests, differential |
| kitty screen preparation: placement reuse by transmission generation, visible-id tracking, insertion-order LRU eviction by count/transmission/decoded quotas | `prepare_kitty_screen`, `CachedKittyImage` | kitty crop/reuse/retention/eviction tests |
| kitty/iterm2 redraw decisions, placement clearing, evicted-image deletions | `do_render_impl` | kitty/iterm2 tests, differential |
| full redraw on first render or size change; line-diff writes; cursor marker/hardware cursor; line resets and width clipping | `do_render_impl` | all render tests, differential |
| OSC 133 zone prefix stripping (one or more zones) | `strip_osc133_zone_prefix` | iTerm2 test, prompt test, differential |
| flash stacking and bottom-slice compositing | `composite_flashes` | flash test, differential |
| driver timers: flash expiry, scrollbar auto-hide, one selection auto-scroll tick per call | `poll` | flash/scrollbar/auto-scroll tests |

## Differential

`tests/differential_tui_alt_screen.rs` drives `oracle/tui-alt-screen-oracle.ts`,
which imports the real `TuiAltScreen`, `Text`, `ScrollView`, `VStack`, `HStack`,
and `Image` from the pinned source and feeds a capturing terminal. It compares
the complete write stream and state (`fullRedraws`, `viewportTop`,
`isFollowingOutput`, scroll tops/follow flags/scrollbar visibility, opened URLs,
paste count) for 23 scenarios: basic scroll and manual scrolling, dock layout,
HStack wheel routing, wheel chaining, keyboard page/line/home/end, OSC 133
prompt jumps, custom search styles, search navigation/focus restore, resize,
flash, mouse selection copy, word/line selection, focus loss, OSC 8 links,
kitty crop, kitty placement reuse, iTerm2 fallback, iTerm2 stale placement
clearing, tmux/screen mouse sequences, preserve-screen stop, right-click paste,
and mouse-disabled start/stop.

## Deviations

- Timers are driver-owned. `poll(now)` advances flash expiry, scrollbar
  auto-hide, and one 50 ms selection auto-scroll tick per call; the TypeScript
  source uses `setTimeout`/`setInterval`.
- Environment reads (`TERM`, `TMUX`, `ZELLIJ`, `STY`, platform) are captured in
  `AltScreenEnvironment` and overridable with `set_environment`; tests do not
  mutate process environment (Rust 2024 requires `unsafe` for `set_var`).
- Kitty cache is an insertion-ordered `Vec<(u32, CachedKittyImage)>` instead of a
  JavaScript `Map`; move-to-end and eviction order match `Map` iteration.
- Selection and scrollbar identities compare `Rc` pointers instead of JS object
  identity.
- `openUrl` and `onRightClickPaste` are `Rc<dyn Fn>` callbacks; exceptions are
  contained with `catch_unwind` to mirror the source's best-effort try/catch.
- The overlay search component is stored both as `ComponentRef` and as a typed
  handle so `setResult` can be called without downcasting.

## Discrepancies (source behavior preserved)

- `applySelection` computes the end row from `selection.start.scrollView.scrollTop`
  (not `selection.end.scrollView`). This is only observable for cross-view
  selections, which `getSelectionBounds` already rejects; replicated literally.
- `refreshSearch` calls `scrollTo(target, { disableFollow: true })` even when the
  target equals the current scroll position, which clears follow-end; replicated.
- Focus-out clears a zero-width selection without requesting a render; a
  completed visible selection is retained. Covered by the focus-loss tests.

## Base fixed cells

`TuiAltScreen::create_fixed_cell` mirrors inherited TS TuiBase.createFixedCell (tui.ts:563–591): retains placement/value, schedules rendering on update (returns false), and unregisters on dispose. It does not use the main-screen immediate-write implementation. Core proof: `extension_ui_fullscreen_factory_uses_base_fixed_cells`.
