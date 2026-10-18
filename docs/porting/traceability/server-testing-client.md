# server/testing/client.ts traceability

Source: `packages/server/src/testing/client.ts`. Rust: `crates/pi-server/src/testing/client.rs`.

| TS item | Rust | Proof |
|---|---|---|
| `ProtocolTestClient` | `ProtocolTestClient` | every unix/session/conformance test |
| `connectUnixTestClient` | `connect_unix_test_client` | same |
| `hello`/`request`/`sendMessage`/`sendBytes`/`sendFragmentedMessage` | same names (`hello_default` for the current version) | fragmented hello, out-of-order, combined-frame tests |
| `next`/`nextFrom`/`waitForClose`/`close` | same names | all waiter-based assertions |
| `receive`/`markClosed`/`fail` | private helpers | exercised by close paths |

## Deviations
- Waiters use a `watch` version counter and scan the retained message list rather than a predicate-waiter set; observable behavior is identical.
- The client shuts down its write half on EOF (Node socket default), which keeps server close deterministic.
