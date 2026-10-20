# tui.ts / tui-main-screen.ts / root-compositor.ts traceability

Sources (pinned revision in `docs/porting/source-lock.json`):

- `packages/tui/src/tui.ts` (2534 lines) → `crates/pi-tui/src/tui.rs`
- `packages/tui/src/tui-main-screen.ts` (586 lines) → `crates/pi-tui/src/tui_main_screen.rs`
- `packages/tui/src/root-compositor.ts` (91 lines) → `crates/pi-tui/src/root_compositor.rs`

Tests: `tests/tui_render.rs`, `tests/tui_shrink.rs`, `tests/tui_focus.rs`,
`tests/tui_cell_size_input.rs`, `tests/tui_overlay_style_leak.rs`,
`tests/overlay_options.rs`, `tests/overlay_short_content.rs`,
`tests/overlay_non_capturing.rs`, `tests/regression_overlay_cjk_boundary.rs`,
`tests/fixed_cell.rs`, `tests/render_region.rs`,
`tests/render_region_safety.rs`, `tests/differential_tui.rs`, and
`tests/differential_tui_alt_screen.rs`.

## Exported symbols

| TS symbol | Rust item | Tests |
| --- | --- | --- |
| `Component` | `Component` trait | every test |
| `TuiInputListenerResult` / `TuiInputListener` | `TuiInputListenerResult`, listener alias | `tui_focus` consumes-focus test |
| `FixedCell` | `FixedCell` | `fixed_cell` |
| `RenderRegionRect` | `RenderRegionRect` | `render_region`, `render_region_safety` |
| `RenderRegion` | `RenderRegion` | `render_region` |
| `FlowRenderRegionLayout` / `FlowRenderRegion` | same names | `render_region` |
| `Focusable` / `isFocusable` | `Focusable`, `is_focusable` | focus/overlay tests |
| `CURSOR_MARKER` | `CURSOR_MARKER` | render-region cursor test, differential |
| `visibleWidth` re-export | `pub use utils::visible_width` | utils differential |
| `OverlayAnchor` | `OverlayAnchor` | `overlay_options` |
| `OverlayMargin` | `OverlayMargin` | `overlay_options` |
| `SizeValue` | `SizeValue` (`Absolute`/`Percent`) | `overlay_options` |
| `OverlayOptions` | `OverlayOptions` | overlay tests |
| `OverlayUnfocusOptions` | `OverlayHandle::unfocus_to(Option<ComponentRef>)` | overlay tests |
| `OverlayHandle` | `OverlayHandle` (`Clone`) | overlay tests |
| `Container` | `Container` | overlay/render-region tests |
| `compositeTuiLine` | `composite_tui_line` | CJK regression |
| `TuiMode` | `TuiMode` | focus reporting / differential |
| `TuiStopOptions` | `TuiStopOptions`; `RendererApi::terminal_stop_output`, `TuiBase::publish_terminal_restore` and shared `frame_stop_sequence` publish exact default stop bytes for cross-thread crash cleanup | stop paths; coding-agent `interactive_crash::worker_restore_matches_normal_ui_stop_bytes` |
| `TUI` interface | `TuiExt` trait (default methods) | all TUI tests |
| `VIEWPORT_TUI`, `ViewportTUI`, `isViewportTUI` | `ViewportTui`, `TuiExt::as_viewport_tui`, `is_viewport_tui` | differential alt-screen scenarios |
| `TuiBase` | `TuiBase` | all TUI tests |
| `beforeTerminalStart` / `afterTerminalStop` hooks | `RendererApi::before_terminal_start` / `after_terminal_stop` | alt-screen start/stop tests, differential |
| `TUIRenderer` / `TUI` alias | `TuiRenderer` | fixed cell/render region/differential |
| `TuiMainScreen` | `TuiMainScreen` | main-screen tests |
| `TuiMainScreenRenderState` / `captureRenderState` / `restoreRenderState` | `MainScreenRenderState`, methods | gap: no listed test; implemented |
| `RootCompositor`, `RootLayoutEntry`, options | same names | `render_region` |

## Branch coverage

| Source branch | Rust item | Tests |
| --- | --- | --- |
| overlay layout: default width, minWidth, maxHeight, percentage, anchors, offsets, margins, clamping | `TuiBase::resolve_overlay_layout`, `resolve_anchor_*`, `parse_size_value` | `overlay_options` |
| overlay compositing: truncate, pad, segment resets, image-line passthrough | `composite_tui_line`, `TuiBase::composite_overlays` | CJK regression, style leak, overlay tests |
| overlay visibility (`hidden`, `visible()` callback) | `is_overlay_visible` | overlay focus tests |
| overlay stack: hide/setHidden/focus/unfocus/isFocused, preFocus retargeting | `OverlayHandle`, `remove_overlay`, `set_overlay_hidden`, `focus_overlay`, `unfocus_overlay` | `overlay_non_capturing` |
| overlay focus restore: eligible/blocked/inactive, clear vs preserve, ancestors, cycles | `set_focus_internal`, `get_visible_overlay_focus_restore`, `resolve_blocked_overlay_focus_resume` | `overlay_non_capturing` |
| focus-order visual stacking | `focus_order` counter, overlay sort | overlay rendering-order tests |
| input: focus reports, OSC 11, color scheme, listeners, cell size, debug key, overlay redirection, release filter | `handle_terminal_input`, `consume_*` | `tui_focus`, `tui_cell_size_input` |
| render scheduling: nextTick/throttle coalescing, force, focus deferral, restart | `request_render`, `flush_pending`, `render_now`, `schedule_render` | `tui_focus`, `tui_render` |
| differential commit: first render, width/height change, Termux height, clear-on-shrink, upward viewport shrink (renderer), deleted viewport (main), first/last changed, appended lines, image reserved rows, extra line clearing, hardware cursor | `commit_rendered_frame`, `full_render`, `position_hardware_cursor` | `tui_render`, `differential_tui` |
| kitty image ids: collect/delete/reserved rows/expand changed range; parameters parsed with `Number` + `Number.isInteger` | `parse_kitty_image_header`, `collect_kitty_image_ids`, `delete_*`, `get_kitty_image_reserved_rows`, `expand_changed_range_for_kitty_images` | `tui_render` kitty tests, `kitty_header_parameters_use_javascript_number_parsing` |
| fixed cells: patch validation, visibility/size/pending/overlay guards, frame sync | `TuiRenderer::write_fixed_cell`, `patch_fixed_cell_line`, `apply_fixed_cells` | `fixed_cell` |
| render regions: placement, generation, descendant mapping, full-width/viewport guards, line diff writes | `TuiRenderer::place_render_region`, `write_render_region`, `get_writable_render_region_rect`, `write_render_region_lines` | `render_region`, `render_region_safety` |
| flow render regions: growth/shrink, scrollback splice, fixed-cell/cursor/region shifting | `build_flow_render_frame`, `plan_flow_*`, `shift_flow_row` | `render_region` |
| line resets and cursor marker extraction | `apply_line_resets`, `extract_cursor_position` | `tui_render`, `render_region` |
| debug redraw log | `log_redraw` + `set_debug_redraw` | `tui_render` debug-log test |
| main-screen capture/restore render state | `capture_render_state`, `restore_render_state` | gap: no listed test |
| RootCompositor flow/bottom layout and callbacks | `RootCompositor::render` | `render_region` |

## Differential

`tests/differential_tui.rs` runs `oracle/tui-core-oracle.ts` with a capturing
terminal for both renderer variants and compares the **complete write stream**,
`fullRedraws`, per-component render/input/focus state, and focused index for 13
scenarios: initial render, middle/append/shrink changes, forced redraw, resize,
cursor marker + hardware cursor, overlay stack + hide, focus-out/in deferral,
fixed-cell updates, clear-on-shrink, renderer shrink, Termux resizes, overlay
percentage/margin/maxHeight, focus steal/restore during input, and repeated
start/stop.

## Deviations

- Components are `Rc<RefCell<dyn Component>>` handles (`ComponentRef`) instead of
  JS object references. `Container` and `TUI` are not subclass relationships;
  `TuiBase` is shared by composition, and `TuiExt` supplies the common surface.
- `OverlayHandle::unfocus` takes no target; `unfocus_to(Some/None)` maps
  `{ target }`. `add_input_listener` returns an id instead of an unsubscribe
  closure.
- `query_terminal_background_color` / `query_terminal_color_scheme` return poll
  handles (`result()`/`is_settled()`) settled by `process_events` instead of
  Promises.
- `TERMUX_VERSION` and `PI_DEBUG_REDRAW` are read once at construction and can be
  overridden explicitly with `set_termux_session` / `set_debug_redraw` so tests do
  not mutate process environment (Rust 2024 makes `set_var` unsafe).
- Focus changes made from inside `Component::handle_input` are applied after the
  handler returns when the component is still mutably borrowed (deferred focus
  queue). Observable focus state after input dispatch matches the source.
- Width/height line-diff rendering, crash logging and throwing, and the renderer
  image pre-clear fallbacks are ported; `PI_TUI_DEBUG` per-render dumps to
  `/tmp/tui` are not ported (debug artifact only).

## Gaps

- `viewport-overwrite-repro.ts` is a manual, assertion-free repro script driven by
  `ProcessTerminal` and sleeps; it is not ported as a test. Its streaming/append
  behavior is covered by `tui_render` append/shrink tests and the differential
  scenarios.
- `captureRenderState` / `restoreRenderState` are implemented but not exercised by
  the listed tests.
- `ViewportTui`/`is_viewport_tui` are implemented and exercised by the
  `tui-alt-screen.ts` slice; the marker trait is a Rust trait object rather than
  a `Symbol.for` property.
- `TuiBase` background/color-scheme query handles have no listed test; parsing is
  covered by the `terminal-colors` slice.
- Debug `/tmp/tui` render dumps and `PI_DEBUG_REDRAW` timestamp text format differ
  (unix seconds instead of ISO timestamps; the reason string is asserted).
