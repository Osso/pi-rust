# utils/abort-signals traceability

Frozen `packages/ai/src/utils/abort-signals.ts`, pin `4f9e6f8a4`. Native `crates/pi-ai/src/utils/abort_signals.rs` uses the shared carrier's literal persistent add_listener/remove_listener API, not bridge tasks or AbortSignal.any. [Carrier contract](../../specs/abort-signals.md).

| Frozen branch | Direct module proof |
| --- | --- |
| No active inputs; missing signal and no-op repeated cleanup | `zero_single_input_identity_and_noop_cleanup_match_frozen_module`: empty and all-undefined lists |
| One active input; original signal identity, cleanup does not detach it | Same test: mixed undefined positions, cleanup before supplied-null abort |
| Multiple inputs; synchronous first reason and first encountered pre-abort | `multi_input_first_reason_is_forwarded_synchronously_without_repair`: default, null/false/empty/native text, arrays, -0/NaN/Infinity, plain Error-lookalike versus Error/DOMException, metadata; initial and live abort ordering |
| Explicit cleanup stops propagation; dropping wrapper with retained signal does not | `cleanup_stops_forwarding_and_discarding_wrapper_does_not`: actual frozen object lifetime versus native wrapper drop; repeated cleanup |
| Synchronous helper requires no asynchronous Rust runtime | `combining_signals_is_synchronous_without_an_async_runtime`: same frozen module/controller behavior from an ordinary Rust test |

`oracle/abort-combine-oracle.ts` imports the actual frozen helper and uses Node AbortController/DOMException. It compares synchronous state, reason branding/value and signal identity. Only error.stack is omitted by user approval; plain JSON stack properties are retained. No clock or reason rewriting.

RED `/tmp/pi-rust-repin-delta-ai/abort-combine-reasons-red.log`: old Tokio bridge had not forwarded on return from abort, and wrapper drop stopped forwarding. Zero/single-input branches remained GREEN. New wrapper forwards the actual reason synchronously and removes listeners only on explicit cleanup; weak ListenerId destruction is not removal. Source callbacks retain the target controller after the result wrapper is discarded. A native post-registration state check closes the check/register gap for cross-thread callers (JS performs those steps synchronously).

Current helper proof: all four direct-module differentials above pass at15085b234 (`/tmp/ai-abort-combine-current.log`). No production change is needed. Current required two-package all-target check and scoped Clippy0 errors/8 existing warnings/0 changed-line warnings remain valid at unchanged Rust revisiond6dd70086 (`/tmp/ai-env-current-{check,clippy}*`). The shared [spec](../../specs/abort-signals.md) already records these helper requirements. Status done for this whole helper; SDK/provider timeout and reason-derived behavior belongs to their own consumer rows, not this helper's completion gate. Older token/bridge-task evidence is retired.

Runtime gap under the 2026-10-25 bar: cross-thread scheduling/interleavings are not compared with Node's atomic check/register steps; the existing native post-registration check remains.
