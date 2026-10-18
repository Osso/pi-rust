# unix traceability

Pinned source: `packages/client/src/unix.ts`. Rust: `crates/pi-client/src/unix.rs`.

| TypeScript symbol / branch | Observable contract | Rust mapping and proof |
|---|---|---|
| `UnixTransportOptions` | `path`, optional `maxPendingBytes` | `UnixTransportOptions` |
| empty path | `TypeError("Unix transport path must not be empty")` | `PiError::InvalidArgument` with exact message; `unix::rejects_invalid_unix_transport_options` |
| path UTF-8 length > 107 (Linux) | `TypeError(... maximum is 107 UTF-8 bytes)` | `String::len()` byte check; same test |
| `maxPendingBytes` default | `DEFAULT_MAX_FRAME_LENGTH * 4` | `DEFAULT_MAX_FRAME_LENGTH as usize * 4` |
| non-safe-integer / non-positive limit | `TypeError(... positive safe integer)` | `usize` excludes negatives/fractions; zero rejected; same test |
| Windows rejection | `"Unix transport is not supported on Windows"` | Not ported; crate currently targets Unix (`tokio::net::UnixStream`). Explicit gap. |
| `createUnixTransportFactory` | Returns a factory | `create_unix_transport_factory(...) -> Result<Arc<dyn ByteTransportFactory>, PiError>` |
| connect before `connect` event | Reject the connection promise | `UnixStream::connect` error becomes `PiError::Io`; `unix::rejects_connection_errors` asserts `ENOENT` (`raw_os_error == 2`) |
| inbound `data` | Forward chunk to `onData` | read task forwards each read; fragmented-byte test |
| `end` / `close` | Orderly `onClose` once | EOF branch calls `on_close`; remote-end test asserts one close |
| `error` | Terminal `onError` | read error branch; no direct fault-injection test (gap) |
| pending byte limit | Reject a chunk before it is queued | synchronous pending accounting in `UnixByteTransport::enqueue`; bound test |
| write ordering | Chunks reach the socket in invocation order | unbounded job channel + single writer task; 4 MiB order test |
| write completion | Resolve only after the OS write/drain | `write_all` completion; bound test awaits both writes |
| local `close()` | Idempotent; suppress terminal callbacks | atomic closed flag, cancels read task via `CancellationToken`, enqueues writer shutdown |
| repeated close | Harmless | atomic swap returns early |

## Differential coverage

Skipped: the module depends on real OS sockets and runtime scheduling. Behavior is covered by three real Unix-socket tests; frame/state parity is covered separately by `client-oracle.ts` through the in-memory transport.

## Gaps / discrepancies

- Local `close()` enqueues shutdown behind already-queued writes, so an in-flight write may complete; the Node source destroys the socket immediately and rejects pending writes. Not exercised by the source tests.
- The Windows rejection branch is not compiled because the crate currently uses Unix-only tokio types.
