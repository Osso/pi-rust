# server/transports/unix/types.ts traceability

Source: `packages/server/src/transports/unix/types.ts`. Rust: `crates/pi-server/src/transports/unix/types.rs`.

| TS export | Rust |
|---|---|
| `UnixListenerOptions` | `UnixListenerOptions` (path, mode, pending-byte limit, graceful timeout, frame limit, error observer) |
| `UnixServerOptions` | `UnixServerOptions` (flat composition of server + listener options) |

Both are validated by `create_unix_listener` / `create_unix_server`; see `tests/server.rs` and `tests/unix.rs`.
