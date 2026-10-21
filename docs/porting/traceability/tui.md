# tui.ts frozen traceability

Reference: immutable `4f9e6f8a4`, `packages/tui/src/tui.ts` (2388 lines).
Rust frozen-mapped implementation: `crates/pi-tui/src/tui.rs::TuiRenderer`, exported as concrete root `TUI`. Frozen has one TUI class, not the later TuiBase/TuiMainScreen hierarchy. Internal later renderers remain for Q2/Q13 consumers; they are not frozen root exports. [Q1 proof](../q1-tui-core-proof.md).

| Frozen exports/function group | Rust representation | Current proof |
|---|---|---|
| Component, Focusable, isFocusable, Container, CURSOR_MARKER | Rc/RefCell component handles, focus trait, container, same marker | rendering/focus/overlay and root constructor tests |
| FixedCell; create/place/clear/update/dispose; patch/apply/write | same handles/state; exact column/viewport/pending-render/overlay/width guards and reset bytes | fixed_cell + full-stream differential |
| RenderRegionRect/RenderRegion; create/map/unmap/place/clear/dispose/request/try/write/prepare | same handles, generation, descendant lookup and full-width/viewport safety | render_region + render_region_safety |
| FlowRenderRegionLayout/FlowRenderRegion; build/splice/shift/plan/apply/getWritable | same layout/state planning, fixed-cell/cursor placement shifts | render_region + empty-flow source differential |
| Overlay options/anchors/margins/percent sizing; resolve/composite functions | same data and segment compositing; frozen min-then-max bound clamp | overlay_options, CJK/style/short-content regressions, oversized-margin source differential |
| focus restoration/ancestor/mounted/retarget/topmost/visibility; overlay handle methods | explicit eligible/blocked/inactive state and focus-order stack | overlay_non_capturing + full-stream focus scenarios |
| start/stop, input listeners, debug handler, focus reports, release/repeat filter | shared core drives regular renderer; exact terminal output | tui_focus, cell-size input, repeat start/stop source scenarios |
| requestRender/schedule/renderPendingState | driver-owned scheduling; forced state reset deferred until execution, not request | full-stream `requestRender(true); stop()` RED/GREEN |
| OSC11/color-scheme queries/notifications, consume methods | pollable handles instead of promises; expired OSC11 queries retain reply order | color parser proof + late-reply full-stream differential |
| line resets, cursor extraction/positioning | same normalization/segment reset/visible viewport cursor search | tui_render, render_region and source byte streams |
| Kitty header/ids/reserved rows/delete/expanded diff range | same integer parsing and placement/deletion behavior | tui_render and kitty-header regression |
| doRender/commit frame; full/visible/differential render | frozen in-place visible repaint after upward shrink; image fallback remains full render; no full render when forbidden by flow update | full-stream shrink/resize/Termux/fixed-cell/overlay matrix + empty-flow RED/GREEN |
| debug redraw/crash logs, PI_TUI_DEBUG dump | default HOME/.pi/agent; ISO headers; exact dump labels/JSON/output payload in /tmp/tui; append failures propagate | differential_tui_debug, differential_tui_debug_failures, tui_render logging cases |

## Representation details

TuiBase/TuiExt/RendererApi composition is Rust scaffolding; it is not asserted to be a frozen exported class hierarchy. OverlayHandle::unfocus_to maps the target option; add/remove listener ids map unsubscribe callbacks. Components mutably borrowed during handle_input receive deferred focus updates before dispatch completes. Timers are driven through process_events/flush_pending; explicit setters support deterministic environment-dependent tests.

Debug dump filenames contain current epoch milliseconds and a nondeterministic base-36 suffix; exact time/random bytes are not compared across processes. File directory, diagnostic fields, pretty/compact JSON payload and terminal bytes are compared. No live host HOME or terminal is used by proofs.

## Pending numeric classification

Frozen lines 1154-1170 parse positive CSI cell-size numbers without a u32 bound. `ESC[6;4294967296;20t` is consumed in frozen (`inputs: []`); Rust currently forwards it because Q2's CellDimensions is u32. Parent explicitly deferred widening that public image API to Q2/user numeric-domain classification. Exact frozen/Rust observations: `/tmp/pi-rust-repin-tui-core/wide-cell-{frozen,rust}.log`. Coverage remains **partial**, not done.

## Verifier closures (2026-10-21)

F1: frame commit owns its RefCell guard and drops it before stop; `verifier_regressions::overwidth_stops_terminal_before_descriptive_panic` asserts descriptive width panic, end-of-content move, cursor-show, terminal-stop bytes, and stopped rendering. Shared main-screen/flow callers no longer retain an outer frame borrow.

F2: `RenderRegion::try_render` attempts the region write without scheduling; `render_region::try_render_*` proves success, disposed/cleared failure and no root-render side effect versus request_render. Deferred legacy handles return false without scheduling.

F3: Container/TuiBase remove only the first Rc-identical child. F4: listener registration deduplicates Rc pointer identity, duplicate subscriptions share an unsubscribe ID, and dispatch follows the live insertion-ordered registry including additions/removals. F5: absent an explicit test override, TERMUX_VERSION is checked dynamically and empty means false. Behavioral proof: `verifier_regressions` duplicate-child, callback-identity/live-dispatch and Termux fixtures derived from verifier probes.
