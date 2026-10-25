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

## Removal of old-only renderer mode (2026-10-22)

Frozen `packages/tui/src` has no `TuiMode`, fullscreen renderer or alternate-screen 1049 sequence. Removed remaining native enum and renderer mode methods, plus stop options/preserve-screen/leading-space branches only used by that deleted renderer. Native extension UI backend no longer invents a mode property; all fixtures use the single renderer. Coding-agent assistant streaming always follows its frozen regular transcript-tail path.

Frozen tui.ts:881–884 enables focus reporting whenever not already enabled, :939–942 disables it when enabled, and :1016–1040 consumes focus reports without a mode gate. Native retains those exact conditions after removing mode dispatch. PTY worker-crash tests no longer branch on unsupported fullscreen settings or assert 1049 output; raw-mode restoration, cursor/paste/keyboard restoration, exit status and crash text assertions remain.

## Representation details

TuiBase/TuiExt/RendererApi composition is Rust scaffolding; it is not asserted to be a frozen exported class hierarchy. OverlayHandle::unfocus_to maps the target option. Numeric unsubscribe handles map to weak callback identities, retained for the TUI lifetime; removal upgrades the callback and compares Rc pointers against active listeners, not insertion generations. Thus any old or repeatedly invoked handle removes a re-registered identical callback without retaining inactive callback captures. Separate insertion IDs preserve live Set dispatch order. Components mutably borrowed during handle_input receive deferred focus updates before dispatch completes. Timers are driven through process_events/flush_pending; explicit setters support deterministic environment-dependent tests.

Debug dump filenames contain current epoch milliseconds and a nondeterministic base-36 suffix; exact time/random bytes are not compared across processes. File directory, diagnostic fields, pretty/compact JSON payload and terminal bytes are compared. No live host HOME or terminal is used by proofs.

## Approved numeric-domain deviation (2026-10-21)

Frozen lines 1154-1170 parse positive CSI cell-size numbers without a u32 bound. `ESC[6;4294967296;20t` is consumed in frozen (`inputs: []`); Rust currently forwards it because Q2's CellDimensions is u32. User approved retaining this numeric-domain gap on 2026-10-21; no public image API widening. Exact frozen/Rust observations: `/tmp/pi-rust-repin-tui-core/wide-cell-{frozen,rust}.log`. Coverage is done within this approved restriction and the F1–F5 closures below.

## Verifier closures (2026-10-21)

F1: frame commit owns its RefCell guard and drops it before stop; `verifier_regressions::overwidth_stops_terminal_before_descriptive_panic` asserts descriptive width panic, end-of-content move, cursor-show, terminal-stop bytes, and stopped rendering. Shared main-screen/flow callers no longer retain an outer frame borrow.

F2: `RenderRegion::try_render` attempts the region write without scheduling; `render_region::try_render_*` proves success, disposed/cleared failure and no root-render side effect versus request_render. Deferred legacy handles return false without scheduling.

F3: Container/TuiBase remove only the first Rc-identical child. F4: listener registration deduplicates Rc pointer identity, duplicate subscriptions share an unsubscribe ID, old/repeated unsubscribe handles remove the Rc-identical callback after re-registration, and dispatch follows the live insertion-ordered registry including additions/removals. F5: absent an explicit test override, TERMUX_VERSION is checked dynamically and empty means false. Behavioral proof: `verifier_regressions` duplicate-child, callback-identity/live-dispatch and Termux fixtures derived from verifier probes. F4b: `stale_input_unsubscribe_removes_reregistered_callback` and `stale_color_unsubscribe_removes_reregistered_callback` execute duplicate registration → second unsubscribe → re-register L → first unsubscribe → dispatch, comparing count 0 against immutable tui.ts via `stale-input`/`stale-color` oracle scenarios. Each also reuses the already-invoked first handle after another re-registration. The earlier duplicate tests did not cover this lifetime ordering.

## Width-error cleanup precedence

[Contract](../../specs/tui-width-errors.md). Frozen :2193–2220 crash-log→stop→exact width Error now returns original `CallbackError` through renderer/commit/render queue/partial-region APIs. The application outer render loop invokes shared uncaught authority. Width panic removed. `raw_frame::width_error_preserves_cleanup_throw_precedence_and_crash_log` actual frozen/native source proof asserts both stop-success exact Error and stop-failure raw-surrogate precedence, file bytes (timestamp excluded), and log-before-stop event. Both genuine native REDs: `renderer-width-both-native-red.log`; targeted GREEN `renderer-width-green-2.log`. Final integration evidence: `editor-result-tui-regression.log` 87/87 (raw frame/terminal/render/differential/region/fixed-cell subsets); lifecycle1/1 and31 distinct coding-agent consumer tests GREEN. Workspace/all-targets0 errors/0 warnings; scoped package all-target clippy0 owned/1697 other warnings; pinned TS/Biome/fmt/inventoryPASS. Parent3175f4f8 merged before final gates, native revisionec373860. [Exact commands/proof boundaries](interactive-external-editor.md#result-api-checkpoint).

## Width-error regression harness (2026-10-25)

`verifier_regressions` still expected a Rust panic after render_now became
fallible. It now reads the returned error and compares the same first line
and terminal-stop bytes against the frozen throw/catch oracle. No panic
catching, relaxed comparison or runtime change is introduced.
