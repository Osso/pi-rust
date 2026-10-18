# promise traceability

Pinned source: `packages/client/src/promise.ts` at `docs/porting/source-lock.json`. Rust: `crates/pi-client/src/promise.rs`.

| TypeScript export | Observable contract | Rust mapping and proof |
|---|---|---|
| `PromiseResolvers<T>` | Holds the promise plus resolve/reject functions | `promise::Promise<T>` + `promise::Resolver<T>` over `tokio::sync::oneshot`; exercised by every handshake and request test |
| `createPromiseResolvers<T>()` | Creates an unresolved promise whose resolve/reject may be called later | `create_promise_resolvers<T>()`; handshake completion/rejection in `connection` tests and request correlation in `requests::correlates_coalesced_out_of_order_responses` |
| resolve after receiver dropped | Source ignores the promise result | `Resolver::resolve` ignores the failed `oneshot::send` |
| receiver dropped before resolution | Source promise never settles | Rust `Promise` maps a closed channel to `PiError::Disconnected`; no direct test (internal invariant) |

## Rust boundary

- `Promise<T>` implements `Future<Output = Result<T, PiError>>`.
- `Resolver<T>` consumes itself on resolve/reject, preventing double settlement.
- `promise` is private; only concrete futures (`ConnectFuture`, `CommandResponse`) are exported.

## Gaps / discrepancies

- `Promise.withResolvers` baseline comment is not applicable; Rust uses the explicit resolver pair.
