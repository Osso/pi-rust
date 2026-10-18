# index traceability

Pinned source: `packages/client/src/index.ts`. Rust: `crates/pi-client/src/lib.rs`.

| TypeScript export | Rust export |
|---|---|
| `PiClient` | `pi_client::PiClient` |
| `PiClientDisposedError` | `pi_client::PiClientDisposedError` |
| `PiDisconnectedError` | `pi_client::PiDisconnectedError` |
| `PiServerError` | `pi_client::PiServerError` |
| `PiSessionDetachedError` | `pi_client::PiSessionDetachedError` |
| `PiSessionOwnershipError` | `pi_client::PiSessionOwnershipError` |
| `AcquireSessionOptions` | `pi_client::AcquireSessionOptions` |
| `PiSessionHandle` / `SessionLease` | `pi_client::SessionHandle` |
| `SessionLeaseMode` | `pi_client::SessionLeaseMode` |
| `ByteTransport` | `pi_client::ByteTransport` |
| `ByteTransportFactory` | `pi_client::ByteTransportFactory` |
| `ByteTransportHandlers` | `pi_client::ByteTransportHandlers` |
| `ConnectionState` | `pi_client::ConnectionState` |
| `ConnectionStateChange` | `pi_client::ConnectionStateChange` |
| `CreateSessionOptions` | `pi_client::CreateSessionOptions` |
| `ListenerErrorHandler` | `pi_client::ListenerErrorHandler` |
| `PiClientOptions` | `pi_client::PiClientOptions` |
| `Unsubscribe` | `pi_client::Unsubscribe` |

Additional Rust-only exports needed to name returned futures/types: `ConnectFuture`, `CommandResponse`, `AttachFuture`, `CreateSessionFuture`, `DisposeFuture`, `SnapshotFuture`, `ReleaseFuture`, `PiError`, `UnixTransportOptions`, `create_unix_transport_factory`, listener aliases, and transport future aliases.

## Gaps / discrepancies

- `SessionLease` is not a separate Rust trait; `SessionHandle` is the concrete lease type.
- `PiClient.connect` static is `PiClient::connect_new` (Rust name collision with the instance method).
