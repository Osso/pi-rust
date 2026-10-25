# components/bordered-loader.ts

Frozen: `coding-agent/src/modes/interactive/components/bordered-loader.ts` at `4f9e6f8a4`.
Rust: `crates/pi-coding-agent-ui/src/modes/interactive/components/bordered_loader.rs`.

| Frozen symbol / branch | Rust representation | Proof |
|---|---|---|
| constructor, supplied theme closures, default `cancellable` (12–43) | sole `new(render, Arc<Theme>, message, options)`; typed plain/cancellable child handles | `differential_bordered_loader`: both modes, distinct supplied/global themes, global theme replacement, unpaired message |
| inherited Container children/render/invalidate (7, 13–43) | `Deref`/`DerefMut` to Container, `as_container` | differential compares rendered children/component lines before/after invalidate and inherited add/remove/clear; private loader still handles cancellation after clear |
| stable cancellable/plain signal getter (44–49) | retained shared AbortSignal | differential stable signal before/after input/dispose |
| onAbort setter, cancellable-only forwarding (51–61) | typed child `set_on_abort`, `handle_input` | differential Escape/Ctrl+C, replacement/removed handler, repeated callbacks |
| dispose branch (63–69) | cancellable `dispose` / plain `stop` | differential before/after dispose; existing `interactive_components_c` loader tests |
| default AbortError producer through CancellableLoader | shared `pi_js_compat::abort` | same differential includes bare controls: DOMException name/message/code 20, first-abort event before callback, repeated callback after first abort |

## Evidence (2026-10-22)

RED `/tmp/pi-rust-repin-lossless-interactive/bordered-loader-red.log`: four bordered cases differ; all lack inherited children, two additionally ignore supplied theme. Bare CancellableLoader controls match. GREEN `/tmp/pi-rust-repin-lossless-interactive/bordered-loader-green.log`: all six cases match exact UTF-16 lines, children, render counts, signal reasons and callback order. Follow-up `bordered-loader-mutations-green.log` adds four inherited-container mutation cases, all GREEN.

TUI argument is represented by the existing render callback boundary. Human message and rendered output stay `JsString`. No carrier rewrite, fallback or new semantic deviation. No source discrepancy found.

2026-10-25 shared-callback correction: `abort_for_input` forwards the existing callback without invoking it under a child borrow. SharedBorderedLoader releases both wrapper/child guards before synchronous invocation, so the share onAbort callback can dispose the loader as frozen does. Main approved this crash fix; the real Escape→share process regression is recorded in [interactive-mode](interactive-mode.md#transfer-commands-2026-10-25).
