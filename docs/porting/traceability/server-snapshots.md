# server/snapshots.ts traceability

Source: `packages/server/src/snapshots.ts`. Rust: `crates/pi-server/src/snapshots.rs`.

| TS item | Rust | Proof |
|---|---|---|
| `ServerSnapshotPublisher` | `ServerSnapshotPublisher` | `tests/sessions.rs::serializes_server_snapshot_revisions`; conformance catch-up test |
| `currentRevision` | `current_revision` | revision assertions |
| `get` | `get` | hello and catch-up tests |
| `broadcast`/`performBroadcast` | `broadcast`/`perform_broadcast` | revision serialization and list metadata events |

## Branches
- No ready connections or closing → no revision increment (`broadcast` early return); exercised indirectly by shutdown/detach tests.
- Revisions serialize through a `tokio::sync::Mutex` queue; the controlled `listModels` test observes exactly `[1, 2]`.
- `get` captures `revision` **before** awaiting sessions/models, matching the TS object-literal evaluation order; this is required for the handshake catch-up test.
- Send failures are reported by `ServerCore::send_message` and disconnect the connection.
