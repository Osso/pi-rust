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
- Setup failure and inner stream failure both terminate with one `error` event whose message is the failure message; the error message carries the requested model's api/provider/id.
- `lazyApi` calls the loader on every stream call (no local cache), matching TS's reliance on the host import cache.
- `fetchDeferred` is exposed only when `capabilities.fetch_deferred`; the method reports `"API does not support deferred responses"` otherwise. Cancellation similarly reports `"API cannot cancel deferred responses"`.
- Inner events are forwarded unchanged; the final result comes from `EventStream::result`.

## Gaps
- TypeScript module-import behavior (`node:module` hooks, bundler opacity) has no Rust analogue and is covered by the TS-only `lazy-module-load.test.ts`; the lazy wrapper contract is ported.
- `result()` is always present in Rust, so the `hasResult` branch collapses to always forwarding the result.

## Discrepancies
- None observed by differential `lazy_stream_error`.
