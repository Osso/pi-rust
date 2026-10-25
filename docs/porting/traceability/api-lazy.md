# lazy stream/api traceability

Source: `packages/ai/src/api/lazy.ts` → `crates/pi-ai/src/api/lazy.rs`.
Tests: `crates/pi-ai/tests/api_lazy.rs`; differential `lazy_stream_error`.

## Symbols → proof
| TS | Rust | Test |
|---|---|---|
| `lazyStream` | `lazy_stream` | `lazy_stream_turns_setup_failure_into_an_error_event`, `lazy_stream_forwards_events_and_result`, differential |
| `lazyApi` | `lazy_api` → `LazyApi` | lazy API tests |
| `forwardStream` | private `forward_stream` | forwarding test |

## Semantics
- Setup runs in a spawned Tokio task; the stream is returned synchronously.
- Setup catches acquisition rejection at frozen lazy.ts:49–53 and uses Error.message/String(error); raw values are not read from an empty CallbackError.message. If String conversion itself rejects, the original conversion rejection reaches the single shared uncaught authority without an invented error event. Inner iteration rejection is outside that catch and reaches the same uncaught authority with its original error branding.
- `lazyApi` calls the loader on every stream call (no local cache), matching TS's reliance on the host import cache.
- Inner events are forwarded unchanged; only a forwarded terminal event settles the outer result. Normal EOF calls `end(None)` and does not inspect/await the inner `result()` (including when that method resolves independently).

## Gaps
- TypeScript module-import behavior (`node:module` hooks, bundler opacity) has no Rust analogue and is covered by the TS-only `lazy-module-load.test.ts`; the lazy wrapper contract is ported.
- Frozen 4f9e6f8a4 has no `hasResult` branch. The former Rust inner-result forwarding is deleted.

## Registered callback prerequisite (2026-10-24)

ProviderStreams raw/simple acquisition returns Result. LazyApi still returns Ok(outer stream) immediately; loaded-module Result belongs to its async setup catch. Codex's direct Simple export rejection now reaches that catch instead of being encoded by the direct module adapter. Success-only module adapters wrap their existing streams in Ok; producer behavior is unchanged. Generic lazy/compat/alias/models consumer regression matrix is GREEN47 at9c739f37. New `provider_lazy_acquisition` invokes actual frozen lazyApi and compares full one-error-event/result values for raw/simple × TypeError/raw UTF16; separate bounded frozen/native child processes establish the catch's conversion rejection is process-uncaught with no invented completion. New-probe GREEN atfec20b12; renewed after parent hook merge0639d2d8 in q15e-stream-postmerge-rejection.

Parent approved moving throw_callback_uncaught unchanged to pi-js-compat::callback_error and re-exporting its old Core path. Pure move0863de71 precedes this usage; the one process crash hook/installation stays in Core-base. All16existing authority tests pass after the move and after parent36aba9a2's diagnostic-hook change (q15e-stream-postmerge-authority); private ignored entrypoints are exercised by active process tests.

## Discrepancies
- None observed by differential `lazy_stream_error`.

## Whole common-pin forwarding audit (2026-10-25)

`lazy_forward_ends_iteration_without_reading_the_inner_result` drives actual frozen `lazyStream` with normal EOF and a separately available result, then compares events and pending outer result against native streams with and without an inner result. Genuine RED was a hanging native iterator at EOF. `provider_lazy_acquisition::frozen_lazy_inner_iteration_failure_is_process_uncaught` drives a frozen rejected async iterator and native failed stream in bounded isolated processes using the installed shared app handler. Genuine RED was an invented native Error event; the fix preserves TypeError branding and emits no completion. Existing raw/simple acquisition failures, load timing/counts and terminal forwarding remain covered. Typed normalizers/import runtime and independent task scheduling are the recorded runtime gap; no widened API.

## Deferred-interface retirement (2026-10-25)

Frozen lazy.ts:62–70 exports only lazyApi(load) with stream/streamSimple. Removed the old-only LazyApiCapabilities type/argument and fetch/cancel-deferred trait methods, request/window/fetch/cancel types and DeferredHandle. Nine native lazy factories, the approved public_index witness and provider_lazy_acquisition fixture now use the single-argument API. No replacement/fallback interface exists.

Retired `lazy_api_fetch_deferred_without_capability_is_an_error_stream` and `faux_core_has_no_deferred_capabilities_at_frozen_pin`; removed unsupported-method assertions from retained Anthropic/Google streaming fixtures. The mixed lazy loader test now checks both real stream methods, complete results, exact invocation log and first-use/load counts, not removed capabilities. The inner failure/result carrier audit is completed below.
