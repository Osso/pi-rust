# lazy stream/api traceability

Source: `packages/ai/src/api/lazy.ts` → `crates/pi-ai/src/api/lazy.rs`.
Tests: `crates/pi-ai/tests/api_lazy.rs`; differential `lazy_stream_error`.

## Symbols → proof
| TS | Rust | Test |
|---|---|---|
| `lazyStream` | `lazy_stream` | `lazy_stream_turns_setup_failure_into_an_error_event`, `lazy_stream_forwards_events_and_result`, differential |
| `LazyApiCapabilities` | same | `lazy_api_loads_on_first_use_and_exposes_only_declared_capabilities` |
| `lazyApi` | `lazy_api` → `LazyApi` | lazy API tests |
| `forwardStream` | private `forward_stream` | forwarding test |

## Semantics
- Setup runs in a spawned Tokio task; the stream is returned synchronously.
- Setup catches acquisition rejection at frozen lazy.ts:49–53 and uses Error.message/String(error); raw values are not read from an empty CallbackError.message. If String conversion itself rejects, the original conversion rejection reaches the single shared uncaught authority without an invented error event. Inner stream failure handling is unchanged by this prerequisite and still needs its own common-pin audit.
- `lazyApi` calls the loader on every stream call (no local cache), matching TS's reliance on the host import cache.
- `fetchDeferred` is exposed only when `capabilities.fetch_deferred`; the method reports `"API does not support deferred responses"` otherwise. Cancellation similarly reports `"API cannot cancel deferred responses"`.
- Inner events are forwarded unchanged; the final result comes from `EventStream::result`.

## Gaps
- TypeScript module-import behavior (`node:module` hooks, bundler opacity) has no Rust analogue and is covered by the TS-only `lazy-module-load.test.ts`; the lazy wrapper contract is ported.
- `result()` is always present in Rust, so the `hasResult` branch collapses to always forwarding the result.

## Registered callback prerequisite (2026-10-24)

ProviderStreams raw/simple acquisition returns Result. LazyApi still returns Ok(outer stream) immediately; loaded-module Result belongs to its async setup catch. Codex's direct Simple export rejection now reaches that catch instead of being encoded by the direct module adapter. Success-only module adapters wrap their existing streams in Ok; producer behavior is unchanged. Consumer/current proof is pending.

Parent approved moving throw_callback_uncaught unchanged to pi-js-compat::callback_error and re-exporting its old Core path. Pure move0863de71 precedes this usage; the one process crash hook/installation stays in Core-base. Existing authority tests must run after migration compiles.

## Discrepancies
- None observed by differential `lazy_stream_error`.
