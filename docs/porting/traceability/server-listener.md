# server/listener.ts traceability

Source: `packages/server/src/listener.ts`. Rust: `crates/pi-server/src/listener.rs`.

| TS export | Rust |
|---|---|
| `PiServerListener` | `PiServerListener` trait (`address() -> Option<String>`, `start`, `close`) |

`address` returns an owned `Option<String>` because the bound path is guarded by a mutex; callers observe the same values (`tests/listener.rs`, `tests/unix.rs`).
`ListenerError`/`ListenerResult` are boxed sendable errors.
