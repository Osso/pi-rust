# transport traceability

Pinned source: `packages/client/src/transport.ts`. Rust: `crates/pi-client/src/transport.rs`.

| TypeScript export | Observable contract | Rust mapping and proof |
|---|---|---|
| `ByteTransport.send(chunk): Promise<void>` | Chunks are delivered in invocation order; failures reject | `ByteTransport::send(&self, Vec<u8>) -> TransportSendFuture`. The method itself is synchronous and enqueues/validates eagerly, matching the source's synchronous async-function prefix; the returned future reports completion/failure. Proof: `connection::*`, `unix::bounds_pending_writes_preserves_order_and_reports_remote_end_once` |
| `ByteTransport.close(): void` | Repeated closes are harmless | `ByteTransport::close`; memory transport asserts one client close in `connection::does_not_restore_connection_when_snapshot_listener_disconnects_during_handshake`, Unix transport uses an atomic closed flag |
| `ByteTransportHandlers.onData` | Arbitrary inbound chunk | `ByteTransportHandlers::on_data`; fragmented hello and coalesced frame tests |
| `ByteTransportHandlers.onClose` | Orderly terminal close | `on_close`; `connection::rejects_pending_requests_on_close...`, Unix remote-end test |
| `ByteTransportHandlers.onError` | Terminal transport failure | `on_error` with typed `PiError`; `connection::rejects_pending_requests_on_transport_errors` |
| `ByteTransportFactory` | Returns a fresh connected transport or rejects | `ByteTransportFactory::create` returning `TransportFuture`; implemented by closures and the Unix factory |

## Rust boundary

- `TransportSendFuture` and `TransportFuture` are boxed `Send` futures; transports must be `Send + Sync`.
- The handler closures are `Arc<dyn Fn ... + Send + Sync>` so connection state can be shared with transport tasks.

## Gaps / discrepancies

- JavaScript may throw synchronously from `send`; the typed Rust signature cannot throw, so all send failures surface through the returned future.
- Non-`Uint8Array` input is excluded by the typed API.
