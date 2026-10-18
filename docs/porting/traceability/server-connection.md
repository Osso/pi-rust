# server/connection.ts traceability

Source: `packages/server/src/connection.ts`. Rust: `crates/pi-server/src/connection.rs`.

| TS export | Rust |
|---|---|
| `ByteConnection` | `ByteConnection` trait (`closed`, `send`, `close`; boxed sendable futures) |
| `ByteConnectionHandler` | `ByteConnectionHandler` trait (`on_data`, `on_close`, `on_error`) |
| `ByteConnectionAcceptor` | `ByteConnectionAcceptor` (`Arc<dyn Fn(Arc<dyn ByteConnection>) -> Box<dyn ByteConnectionHandler>>`) |
| `ConnectionStage` | `ConnectionStage` enum with `is_terminal` |
| `ConnectionState` | `ConnectionState` (id, connection, decoder, session ids, stage, disconnected, handshake completion, timeout handle, handshake watch) |
| `isTerminalConnection` | `is_terminal_connection` / `ConnectionState::is_terminal` |

## Branches
- Terminal = disconnected or closing/closed: used by `PiServer::receive`/`dispatch_message`; exercised by every close/timeout conformance test.
- `MaybePromise` close is normalized to an always-async future.
- Handshake coordination uses a `tokio::sync::watch` signal instead of a stored promise; requests received during `handshaking` await that signal (`tests/conformance.rs::accepts_a_transport_fragmented_framed_cbor_hello`, catch-up test).

## Gaps
- No `AbortSignal` exists in the server package, so no `CancellationToken` mapping is needed.
