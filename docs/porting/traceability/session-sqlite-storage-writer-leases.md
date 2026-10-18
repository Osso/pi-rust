# Traceability: `packages/session-backends/sqlite-node/src/sqlite/storage/writer-leases.ts`

Rust: `crates/pi-session-sqlite/src/sqlite/storage/writer_leases.rs`. Tests:
`writer_leases.rs`, differential.

| TypeScript | Rust | Tests | Notes |
|---|---|---|---|
| `WriterLease` | `WriterLease { owner_id: String, fence, expires_at_ms }` | writer-leases | Owner ids are UUIDv7 strings, like the source. |
| `acquireWriterLease` | `acquire_writer_lease` | writer-leases | `INSERT ... ON CONFLICT DO UPDATE ... WHERE expires_at_ms <= now RETURNING`; `None` while live. |
| `renewWriterLease` | `renew_writer_lease` | writer-leases | Updates the lease expiry in place on success. |
| `releaseWriterLease` | `release_writer_lease` | writer-leases | Deletes only this owner/fence. |
| `deleteWriterLease` | `delete_writer_lease` | writer-leases, conformance | |

Two-process behavior (second writer rejection, stale fence takeover, heartbeat renewal) is
proven with two real repositories/connections in `writer_leases.rs`.
