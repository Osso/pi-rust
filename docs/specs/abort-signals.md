# Reason-bearing abort signals

One shared native signal carrier matches Node AbortController/AbortSignal and the frozen `ai/src/utils/abort-signals.ts` consumers at `4f9e6f8a4`. [Semantics and proof](../porting/traceability/shared-abort.md).

## What it must do

- [x] Distinguish undefined-before-abort from supplied null, false, numbers, native UTF-16 text, arrays and objects; retain only the first reason.
- [x] Preserve Error versus DOMException versus plain-object branding without inferring a class from name/message fields.
- [x] Default omitted/undefined abort reasons to the AbortError DOMException with Node's message/code; timeout produces TimeoutError DOMException.
- [x] Forward any's first reason synchronously; deliver one-shot listeners in source order and honor removal during dispatch.
- [x] Retained add_listener registrations survive dropped ListenerId owners until abort/removal/signal destruction; remove_listener takes the identity explicitly. Drop-scoped AbortSubscription remains distinct.
- [x] Await cancellation without lost wakeups; retain composite sources and release subscriptions/timers with their owners.
- [x] Validate timeout's binary64 integer/uint32 domain with exact Node errors; normalize accepted oversized timer delays like Node.
- [x] Share NativeNodeError from pi-js-compat::node_error, with no old core re-export. Preserve native human fields and arbitrary metadata; omit error.stack.
- [x] Frozen combineAbortSignals preserves zero/single-input identity and forwards supplied reasons synchronously, including first pre-aborted input order.
- [x] Its explicit cleanup removes listeners; discarding the wrapper while retaining its signal does not stop propagation. Combining supplied signals requires no async runtime.
- [x] Codex SSE header fetch validates the timeout before fetch, receives the literal combined signal, and detaches combined listeners after header completion. Body completion never fabricates a signal abort; header timeout exposes TimeoutError DOMException to fetch and the frozen caller-facing timeout message.

- [x] Anthropic OAuth token requests use an independent 30s timeout signal through fetch/body; reason-derived wrapping retains TimeoutError/code23 and native Error metadata/raw value formatting. Rust trait caller cancellation is not substituted for that frozen deadline.

- [x] OpenAI/Anthropic SDK fetch wrappers retain their own signal and default abort reason, deliberately discard caller custom reasons as pinned SDKs do, clear header timers after fetch, and preserve caller listeners after completion. Their connection timeout classification consumes branded fetch rejections and native cause/value conversion.

- [x] GitHub Copilot model availability gets an independent 5s TimeoutError signal through fetch/body and propagates its DOMException rejection unchanged.

- [x] Bundled Codex hosted tools expose custom timeout Error reasons and synchronous caller reasons. Web search rejects pre-abort before invoking its worker and converts only non-Error reasons to its generic abort message; image generation still invokes its worker. Clear timers at completion without breaking retained caller forwarding.

- [x] Google SDK child signals discard caller custom reasons but preserve lifecycle; send/body non-Error failures use JSON text. Mistral Request signals forward exact reasons synchronously, omit default deadlines for supplied callers, and classify request rejection using structural fields plus actual built-in Error constructor identity.

- [x] Bedrock Smithy header failures use Request aborted for Error/DOMException or falsey reasons and native String(reason) for truthy raw values. Cancellation after headers surfaces the real Node body failure aborted; the final post-stream caller check remains separate.

- [x] Auth callbacks, credential operations, queued mutations and storage locks retain the actual signal reason instead of manufacturing a generic cancellation. Legacy OAuth refresh catches use Error.message or shared JS String(value), preserve the original cause, and propagate conversion failures. [Boundary proof](../porting/traceability/auth-types.md).

## How it works

- [Carrier API, consumer audit and proof](../porting/traceability/shared-abort.md).

## Implementation inventory

- `crates/pi-js-compat/src/abort.rs` — shared controller, signal, branded reason and subscription capabilities.
- `crates/pi-js-compat/src/node_error.rs` — single native error-record authority.
- `crates/pi-agent-core/src/harness/types.rs` — native IO rejection uses the shared error type.
- `crates/pi-coding-agent/src/core/model_catalog_cache.rs` — shared error import only in this slice.
- `crates/pi-ai/src/utils/abort_signals.rs` — literal multi-input listeners and explicit cleanup.

## Tests asserting this spec

- `crates/pi-js-compat/tests/abort_signal.rs` — direct Node/frozen-consumer differential and native threaded wakeup/lifetime tests.
- `oracle/abort-signal-oracle.ts` — Node builtins plus direct frozen combineAbortSignals import.
- `crates/pi-ai/tests/differential_abort_combine.rs` and `oracle/abort-combine-oracle.ts` — exported helper's state/reason/identity, explicit cleanup and dropped-wrapper lifetimes.

## Known gaps (current cycle)

- [x] Migrate production consumers off tokio_util CancellationToken; no compatibility aliases or token fallback.
- [ ] Propagate supplied reasons through Node IO nested causes and coding/provider consumers.

## Out of scope

- Invalid JS runtime input types excluded by the typed Rust API; JSON/native Error/DOMException reasons remain supported.
- JS object identity/mutation-through-returned-reference and error.stack text follow the approved deviations in AGENTS.md.
