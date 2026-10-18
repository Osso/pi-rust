# server/testing/server.ts traceability

Source: `packages/server/src/testing/server.ts`. Rust: `crates/pi-server/src/testing/server.rs`.

| TS export | Rust | Proof |
|---|---|---|
| `TestServerOptions` | `PiServerOptions` (service passed separately) | `tests/listener.rs` |
| `TestServer` | `TestServer { server, service }` | `tests/listener.rs` |
| `createTestServer` | `create_test_server` / `create_test_server_with_service` | listener composition tests |
