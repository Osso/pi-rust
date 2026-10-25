# components/cancellable-loader.ts traceability

Frozen `4f9e6f8a4`, `packages/tui/src/components/cancellable-loader.ts` (40 lines).
Rust: `crates/pi-tui/src/components/cancellable_loader.rs`.

| Frozen export / branch | Rust representation | Proof |
|---|---|---|
| CancellableLoader extends Loader, constructor (9–17) | composes Loader; native message/colors/options | `differential_components::cancellable_loader_matches_typescript` |
| signal getter and aborted (20–26) | shared `pi_js_compat::abort::AbortSignal`, `is_aborted` | `differential_bordered_loader` bare controls; stable signal across repeated input/dispose |
| cancel key → abort then onAbort (29–35) | shared AbortController `abort(None)` before callback | same differential: Escape/Ctrl+C, exact default DOMException AbortError/message/code 20, listener-before-callback, one abort event but repeated callbacks, replacement/removal of callback |
| dispose (38–40), inherited rendering/message/indicator/stop | Loader delegation | component differential; bare controls dispose and input after dispose |

Q13 (2026-10-22): CancellationToken mapping is retired; the reason-bearing carrier is reused without projecting/defaulting supplied reasons. GREEN `/tmp/pi-rust-repin-lossless-interactive/bordered-loader-mutations-green.log` includes six loader/control cases plus four inherited bordered-container mutation cases. Loader frame advancement remains driver-owned polling (see [loader](loader.md)); empty frames here do not animate. No source discrepancy found.

2026-10-25 shared-callback correction (main approved): native `abort_for_input` returns the existing callback after aborting; ordinary `handle_input` invokes it synchronously. Shared BorderedLoader callers can release both RefCell guards before invocation. A real Escape→share cancellation process regression reproduces the former panic when onAbort disposes its own loader. Abort-before-callback and repeated-callback semantics are unchanged; [integration proof](interactive-mode.md#transfer-commands-2026-10-25-in-progress).
