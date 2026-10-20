# Extension examples M: forms and overlays

Pinned source root: `/home/osso/.worktrees/pi-rust-source-532f7d12/packages/coding-agent/examples/extensions/`.
Audited against Rust base `1e27e41f8498c4cca42b353bae759c4494a84b7e` on 2026-10-20.
All three modules are **stopped, not ported**. No native factories, stubs, or default registrations were added. Coverage remains `todo`.

## Missing custom-component editor bridge

`question.ts:73–89` creates an `Editor` inside `ctx.ui.custom`; input and render delegate to that instance at lines 115 and 193. `questionnaire.ts:110–130` does the same, with input/render at lines 199 and 342. These are not replacement main-editor factories.

Rust `core/extensions/types.rs:262` requires `ExtensionUiComponent: Send + Sync`; `ExtensionUiCustomFactory` at lines 400–402 returns `Box<dyn ExtensionUiComponent>`. The existing main-editor factory at lines 375–385 can return `Rc<RefCell<dyn EditorComponent>>`, but that is a different host operation and does not return a custom form.

`pi-tui/src/components/editor.rs:245–268` stores `Rc<dyn EditorHost>`, `Rc<dyn EditorFixedCell>`, style functions and other local handles. A component containing even `Mutex<Editor>` fails the custom-component bound: `Mutex<T>` cannot make a non-`Send` editor `Sync`. The merged `ExtensionUiTui: EditorHost` implementation (`types.rs:330–342`) supplies fixed cells but does not resolve this component ownership boundary.

Compile/API RED: `cargo test -p pi-coding-agent --test example_question`, exit 101, nine E0277 diagnostics. Log: `/tmp/pi-rust-ext-examples-m/01-question-editor-api-red.log`; reproducer: adjacent `example_question-probe.rs`. This is compile evidence, **not** a behavioral test pass. Probe removed from the repository after recording diagnostics.

Required core surface: a supported custom-component path capable of owning/delegating to the existing UI-thread `Editor`. No TLS registry, unsafe wrapper, replacement editor, or local reimplementation was introduced. Main explicitly confirmed this STOP classification.

| Module / exported registration | Source branches still missing | Rust / proof |
|---|---|---|
| `question.ts` default; tool `question` | UI/no-UI and empty-option errors; bounded selection; custom editor submit/blank/back/cancel; wrapped ANSI form; result/details and both renderers | No implementation; shared editor API RED above |
| `questionnaire.ts` default; tool `questionnaire`; private `errorResult` | UI/empty errors; defaults; single/multi tab navigation; insertion-ordered answer replacement; custom editor/blank/back; incomplete/complete submit; partial cancel; wrapped ANSI form and both renderers | No implementation; same custom-editor boundary |

The non-editor branches are not shipped independently: the assignment requires whole modules and STOP on a missing API. Key-sequence, width/focus/cursor differential and mutation proofs remain pending, not waived.

## Missing overlay host operations

1. **Nested overlays:** `overlay-qa-tests.ts:957`, `:1057`, and `:1278` call `tui.showOverlay` inside passive/focus/streaming controllers and retain returned handles. Rust `ExtensionUiTuiBackend`/`ExtensionUiTui` (`types.rs:288–342`) expose fixed cells, rendering requests, dimensions and mode, but no `show_overlay` operation. Compile/API RED: `cargo test -p pi-coding-agent --test example_overlay_qa_tests`, exit 101, E0599 `no method named show_overlay`. Log: `/tmp/pi-rust-ext-examples-m/02-overlay-nesting-api-red.log`; reproducer: adjacent `example_overlay_qa_tests-probe.rs`. Probe removed; no test runtime or TS oracle was executed.
2. **Handle focus query:** TS lines 1107 and 1419 call `OverlayHandle.isFocused()`. Rust `ExtensionUiOverlayHandle` (`types.rs:405–411`) exposes `hide`, `set_hidden`, `is_hidden`, `focus`, and `unfocus`, but lacks `is_focused`. Source-inspection evidence only; no runtime parity claim.
3. **Handle callback delivery:** TS registration lines 246–260 provides `onHandle` to save a handle; `ToggleDemoComponent` lines 895–906 uses it to hide/show the overlay. Rust declares `ExtensionUiCustomOptions.on_handle` (`types.rs:454–461`), but `interactive_mode.rs:10826–10875` adapts the component, resolves overlay options and calls `self.ui.show_overlay(...)` without retaining/delivering the returned handle. Existing editor/overlay traceability also explicitly excludes callback delivery. Source-inspection evidence, not a missing declaration or a passing runtime test.

`overlay-qa-tests.ts` default registers all 13 commands below. The entire module is stopped; the rows distinguish direct missing-API consumers from otherwise independent scenarios deliberately not shipped as a partial module.

| Registered command (TS line) | Scenario / component still missing | Blocking/proof status |
|---|---|---|
| `overlay-animation` (32) | 30-FPS frame/FPS counters, bouncing bar, spinner, HSL RGB color cycling, timer disposal | Whole-module STOP; not tested |
| `overlay-anchors` (43) | Nine anchors; next wrap, confirm notification, cancel | Whole-module STOP; not tested |
| `overlay-margins` (81) | Five scalar/edge margins and signed offsets; next wrap/close | Whole-module STOP; not tested |
| `overlay-stack` (115) | Three delayed overlapping layers, concurrent completion and ordered results | Whole-module STOP; not tested |
| `overlay-overflow` (159) | Streaming subprocess stdout/stderr/close, scroll bounds, truncation, kill/dispose | Whole-module STOP; not tested; no process spawned |
| `overlay-edge` (170) | Right edge with zero right margin | Whole-module STOP; not tested |
| `overlay-percent` (181) | Five row/column percentage cases; next wrap/close | Whole-module STOP; not tested |
| `overlay-maxheight` (217) | 21 rendered lines clipped to maxHeight 10 | Whole-module STOP; not tested |
| `overlay-sidepanel` (228) | 25% width/minWidth 30, visibility threshold 100, menu navigation | Whole-module STOP; not tested |
| `overlay-toggle` (246) | onHandle, hide/restore timer, count and re-entry guard | Missing callback delivery; source evidence above |
| `overlay-passive` (264) | Nested passive timer, active input/debug/focus, cleanup | Missing nested overlay API; E0599 above |
| `overlay-focus` (276) | Three overlapping inputs, focus order, Tab/Shift+Tab, dismissal/errors, cursor markers | Missing nested overlay and focus-query APIs |
| `overlay-streaming` (288) | Controller/three-panel focus loop, simulated streaming timestamps, input, cleanup | Missing nested overlay and focus-query APIs |

`BaseOverlay`, all 13 scenario components/controllers, `TimerPanel`, `FocusPanel`, `StreamingInputPanel`, `sleep` and `hslToRgb` remain unported. No ANSI/placement/results/mutation claims are made for these branches.

## Proof and decisions

- Ledger: `/tmp/pi-rust-ext-examples-m/ledger.md`. Both compile probes are archived outside the repository; neither remains as a failing test or production stub.
- No GREEN, differential, or mutation proof: implementation stopped before production changes. No node/tsx/pi binary, oracle, model, or live state was invoked. Only the two authorized targeted Cargo commands were used for API evidence.
- No new dependencies or semantic deviations. Native embedding, value/order/error parity, omission of JS stack frames and String-boundary U+FFFD remain the required contract for the eventual port.
- Follow-up requires core APIs first, then full RED→GREEN key-driven differential and mutation proofs for each module. Main owns the core slice; this batch does not alter shared host APIs.
