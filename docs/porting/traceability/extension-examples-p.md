# Extension examples P: overlay QA

Pinned source: `532f7d12`, `packages/coding-agent/examples/extensions/overlay-qa-tests.ts` (1–1451). Native embedding only; no default loading. Factory: `overlay_qa_tests_extension(&ExtensionApi)` in `crates/pi-coding-agent/examples/extensions/overlay_qa_tests.rs`; retain companion `overlay_qa_tests/` directory. No new dependencies or public APIs. Integrated fallible factories from master `0daec051`.

## Capability matrix

All rows use `tests/example_overlay_qa_tests.rs::thirteen_commands_match_pinned_runtime_options_keys_and_ansi_frames` through a real `ExtensionRegistry`, factory loader, `ExtensionRunner`, command context, and UI-local custom factory. `oracle/example-overlay-qa-tests-oracle.ts` executes the complete pinned file through the real TS loader/runner with real pi-tui Input/width/key helpers. Only UI host, timers, clock and child-process I/O are fixture seams.

| Exact TS source / command | Native implementation | Behavioral proof / remaining scope |
|---|---|---|
| 31–40, 764–855, 856–876: `overlay-animation`, `AnimationDemoComponent`, `hslToRgb` | `Model::{start_timer,tick,input,cleanup}`, `render::{animation,hsl_to_rgb}` | Initial/1100ms frames, FPS, sine bar, spinner, RGB bytes, narrow/wide render, Ctrl+C and stopped timer; full ANSI + render-request differential. Saturation-zero HSL branch is private/unreachable from this example and has no independent fixture. |
| 43–78, 336–374: `overlay-anchors`, `AnchorTestComponent` | `overlay_options`, command loop, `Model::input`, `render::render` | Nine positions plus wraparound; Space/Right; confirm notification, Escape; narrow frames. Other cancel alias shares tested Ctrl+C matcher. |
| 81–112, 375–414: `overlay-margins`, `MarginTestComponent` | Same loop/options/render | All five configurations, scalar margins projected to four equal edges, signed offsets, wrap, narrow frames and close. |
| 115–156, 415–459: `overlay-stack`, `StackOverlayComponent`, `sleep` | `stack`, `render::stack` | Three delayed overlapping calls; Enter/Escape/Ctrl+C; exact registration-order results and notifications, colored 12-line frames; out-of-order completions. Separate paused-clock `stack_mounts_later_layers_at_absolute_four_hundred_ms_deadlines` constructs/closes each actual component and records mount times 0/400/800ms relative to command start despite early 200ms/100ms completions. Relative-sleep mutation yields 0/600/1100ms and fails; no wall-clock upper bound includes registry/thread setup. Widths ≤3: `stack_renders_clamped_fill_rows_at_width_three` asserts the post-pin fixed-TS frame (below). |
| 159–167, 460–592: `overlay-overflow`, `StreamingOverflowComponent` | `overflow::Process`, `overflow::read`, unchanged decoded shell template `overflow.sh.txt` | Real private subprocess, explicit process.env forwarding, stdout chunk splitting including trailing partial chunks, empty filtering, ANSI/OSC8/CJK/emoji truncation, trimmed/styled stderr, auto-scroll and bounded Up/Down, close/Done status, SIGTERM/dispose, actual PID reaping, late output ignored after dispose. Full frames/request counts compare with source callbacks receiving the same chunks. Launch/read/wait/permission failures are not independently exercised; see proof limitations. |
| 170–178, 593–627: `overlay-edge`, `EdgeTestComponent` | `overlay_options`, `render::render` | Exact right-center, width 40, right margin zero, frame and close. |
| 181–214, 628–667: `overlay-percent`, `PercentTestComponent` | `PERCENT`, loop/options/render | Five row/column percentage cases plus wrap; next/close; narrow frames. |
| 217–225, 668–702: `overlay-maxheight`, `MaxHeightTestComponent` | options + `render::render` | Exact maxHeight 10 option, all 21 component lines, Escape. Clipping itself belongs to core pi-tui, not this host fixture. |
| 228–243, 703–763: `overlay-sidepanel`, `SidepanelComponent` | `Model::menu_input`, `render::sidepanel` | Width 25%, minWidth 30, threshold false at 99 / true at 100; Up at zero, navigation, Enter redraw and close. `sidepanel_saturates_at_both_menu_ends_against_pinned_source` reaches the fifth item, presses Down twice more, returns to zero and presses Up again; every frame compares with the pin. |
| 246–261, 879–938: `overlay-toggle`, `ToggleDemoComponent`, `globalToggleHandle` | one UI-thread-local `TOGGLE_HANDLE`, `on_overlay_handle`, `Model::toggle` | Shared global semantics across two factories: first component hides/restores second handle; count, repeated-key guard, 1000ms restore and redraw. Native local bridge is the approved equivalent of source onHandle retention. |
| 264–273, 941–1032: `overlay-passive`, `PassiveDemoController`, `TimerPanel` | `panels::mount`, `Model::{input,tick,cleanup}`, panel render | Passive nested options, focused/debug input counts and UTF-16 lengths, BMP typing, astral rejection, backspace, 61-second rollover, timer cancellation and nested hide. |
| 276–285, 1034–1250: `overlay-focus`, configs/controllers/panels | `panels::{mount,Panel}`, `Model::{focus_first,focus_next,dismiss,close}` | Three exact positions, nonCapturing; initial focus; real Input Unicode, arrows, backspace, key-history tail, cursor markers; forward/reverse wrap; per-panel dismissal, final close, Ctrl+C, dim/focused borders and current-label query. `focus_dispose_then_ctrl_c_does_not_complete` and the thirteen-command lifecycle audit compare Done call counts, disposal then Ctrl+C, and repeated controller Ctrl+C. Focus alone guards close; disposal marks it closed without Done. Private invalid-index/not-open panic paths are implemented but not separately invoked by valid command scripts. |
| 288–300, 1251–1451: `overlay-streaming`, controller/input panels | panel mount/input/render, `Model::{cycle_streaming,tick,close}`, `render::streaming` | Controller → A → B → C → controller; BMP typing, astral rejection, backspace, per-panel close; 500ms lines, eight-line retention, stopped timer, label `?` after clearing active panels; full frames/request counts. |
| 305–334: `BaseOverlay` | `render::{boxed,row}` | Every command's ANSI border/title/truncation/padding output; widths 1, 3, 7, 9, 12 and 60 across applicable fixtures. Width 3 stack compares with the post-pin fixed source, not the pinned oracle. |

## Fidelity follow-up

`/tmp/pi-rust-ui-fidelity/ledger.md` records exact revisions, command diffs and logs.
F1: original disposal and repeated-close tests fail Done counts; U1: original
wait-only completion publishes Done before stdio EOF. P1: broad padding collapse
fails exact-padding and missing-padding tests. `overlay-mutations.log` rejects
all four regressions plus fifth-item saturation and relative stack sleeps (seven
failing behavioral tests). All mutations restored before GREEN. The all-command
lifecycle audit compares both sequences for all thirteen commands; no additional
close/dispose mismatch found. Final `readable-oracles-green.log`: **9 passed**
after the async-factory migration and master integration; every current overlay
proof remains active. The timing fixture uses the paused command runtime and
constructs the actual stack components, not private UI-thread elapsed time.

## Proof ledger

`/tmp/pi-rust-ext-examples-p/ledger.md` lists exact commands, revisions and retained `NN-*.log` output. Final development proof at `15068fe8`: `cargo test -p pi-coding-agent --test example_overlay_qa_tests`, **2 passed**, 24 complete pinned-runtime differential requests plus the native stack timing regression. Later documentation-only commits do not invalidate this code proof. No check, clippy, workspace suites, live providers, live config/DBs, or delegated agents/model CLIs were used.

RED evidence: missing module (01); streaming post-close label (08); source-global toggle target (10); incorrect early-close stack mounting deadline (11); missing overflow scroll redraw (13); child omitted process.env overlay (15). GREEN: 07 initial 13 commands; 09 expanded timers/wrap/input; 12 shared-toggle and stack fixes; 16 complete subprocess and render-request proof. Setup failures (02–06,14) are not behavioral passes. Initial `cargo fmt` failed on fixture escape syntax before the first commit; corrected in `9eb93d6d`; subsequent commits used successful fmt/check. No failed run is claimed as proof.

## Common-pin native streaming input (2026-10-22)

Frozen `4f9e6f8a4` `overlay-qa-tests.ts:1409–1428` appends one UTF-16 unit, removes one unit on backspace, and passes typed text directly to width truncation. No scalar projection or render-slice boundary exists there. Native `panels.rs::streaming_render` now constructs `JsString` directly from its stored units. `streaming_panel_retains_raw_typed_units_until_backspace` exercises the real extension command/factory and nested panel: both U+D800 and U+DC00 must remain in rendered output, then disappear on backspace. RED `/tmp/pi-rust-repin-lossless-interactive/test-overlay-typed-red.log` observed U+FFFD. Historical passive differential projection below remains a separately recorded old fixture boundary, not evidence for streaming replacement.

Common-pin oracle setup uses frozen `core/model-registry.ts::ModelRegistry.inMemory` (:374–375); retired `test/model-runtime-test-utils.ts` does not exist. At parent `fd79d875`, five differential tests failed before a response because of that import. After replacement, eight tests passed; two exposed concurrent process fixtures restoring/removing each other's process-global PATH/control variables. A fixture-lifetime mutex now serializes those environment mutations. These were oracle setup and test-state defects, not frozen component divergences; assertions remain unchanged.

## Boundaries and pending cases

- String boundary: request UTF-16 unit `[55296]` projects to U+FFFD before the existing String component API, matching [compatibility](../../specs/compatibility.md#out-of-scope). The passive fixture renders the replacement character; source receives `toWellFormed()` at the same boundary. Ordinary text/frames are unchanged.
- Streaming timestamps use the existing en-US/local-zone formatter. Only `[h:mm:ss AM/PM]` is replaced by `[<local-time>]` for differential comparison. For a one-digit hour only, normalization first adds its leading zero and removes exactly one padding space before the closing styled border (missing space fails). Two-digit hours and all remaining padding stay exact. Unit fixtures cover all twelve hours and reject extra/missing padding; no arbitrary padding runs collapse. This made the differential fail during those hours until 2026-10-20. Surrounding ANSI, line numbers and retained order remain exact. Source virtual time and native wall time are deliberately distinct, not an exact clock-value claim.
- Source stdout callbacks split each received chunk independently. That behavior, including separate partial/tail lines, is preserved rather than silently buffered into logical lines. Actual OS chunk partitioning and invalid-byte decoding are not claimed byte-for-byte across Node/Tokio.
- Overflow's private executable fixture replaces only the process producer; Rust really spawns, reads, signals and reaps it. The source callback fixture does not execute the 30-second shell animation. Native production keeps its decoded source shell template. `overflow_done_waits_for_final_output_after_child_exit` compares immediate final-write/exit frames and an exited child with inherited stdout held open until a final write. Done is forbidden before stdio closes and every Done frame must contain final output. The completion task awaits both readers after reaping; disposal cancels readers without publishing Done. Process-launch/read/wait/signal-error branches lack independent fixtures; panic messages currently include operation context and do not claim JS error-name parity.
- This is not a compositor/PTY acceptance proof: component frames, handle state and exact passed options are observed through a deterministic UI host. Actual placement, clipping, focus restoration and adapter callbacks retain [core UI](core-ui-overlay.md) proof. The host fixture owns UI-local nested components on its UI thread; its test-only TLS is not production component storage.
- **Post-pin source fix:** pinned `StackOverlayComponent.render` calls `"░".repeat(innerW - 2)` with `innerW = max(1,width - 2)`; widths ≤3 throw `RangeError: Invalid count value: -1`. User chose a TS root fix on 2026-10-20: TS `4f9e6f8a4` clamps the count at zero (`test/overlay-qa-tests-example.test.ts` asserts the 12-line frame). Native `saturating_sub` matches; `stack_renders_clamped_fill_rows_at_width_three` asserts the same ANSI-stripped frame for all three layers. Mutation `inner - 2` panics with subtract overflow (`/tmp/overlay-w3-mutant.log`). Carry into the later re-pin ([compatibility](../../specs/compatibility.md)).

## Subagent child flags re-pin (2026-10-25)

The frozen example passes only the agent configuration's model flag, never
the parent session model/thinking level. Native dispatch no longer captures
those parent values or emits inherited flags. RED records native inherited
model/high versus frozen null/null in actual child invocation journals.
The differential still compares both journals; concrete expectations now
assert the frozen-probed null values and retain explicit model override checks.
