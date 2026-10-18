# server/transports/unix/preset.ts traceability

Source: `packages/server/src/transports/unix/preset.ts`. Rust: `crates/pi-server/src/transports/unix/preset.rs`.

| TS export | Rust | Proof |
|---|---|---|
| `createUnixServer` | `create_unix_server` | all unix/session/conformance integration tests |

`UnixServerConfigError` wraps listener and server option errors; option-validation tests in `tests/server.rs` cover both variants.
