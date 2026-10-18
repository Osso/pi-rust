# server/transports/unix/listener.ts traceability

Source: `packages/server/src/transports/unix/listener.ts`. Rust: `crates/pi-server/src/transports/unix/listener.rs`.

| TS item | Rust | Proof |
|---|---|---|
| `validateUnixSocketPath` | `validate_unix_socket_path` | overlong path tests |
| `UnixListener` | `UnixListener` | `tests/unix.rs` (5 tests) |
| `UnixByteConnection` | `UnixByteConnection` | `tests/unix_connection.rs` |
| `createUnixListener` | `create_unix_listener` | server/unix tests |
| `getOwnedBindPath` | `get_owned_bind_path` (SHA-256 prefix) | exercised by every unix start |
| `removeStaleSocket` | `remove_stale_socket` | live/stale/non-socket tests |
| `isSocketLive` | `is_socket_live` | live listener rejection |
| `cleanupOwnedSocket` | `cleanup_owned_socket_at` | replacement-inode test |
| `setSocketMode` | `set_socket_mode` | 0o600 assertion |
| `closeNetServer` | accept-task abort + connection closes | graceful close tests |

## Branches
- Path validation (empty, >107 bytes on Linux / >103 elsewhere, private bind path).
- Mode 0..=0o777, `maxPendingBytes >= maxFrameLength + 4`, graceful timeout 1..=2_147_483_647.
- Non-socket path is never unlinked; live socket is never unlinked; stale socket is renamed, identity-checked, and removed.
- Replacement inode is preserved during shutdown.
- Nested parent directories are created with mode 0o700; socket mode is applied to the public path.
- Pending-byte limit rejects oversized queues; writes serialize; final chunk queues behind pending writes; graceful timeout forces closure (`queues_a_final_protocol_error_behind_pending_output_before_closing`).
- Read EOF half-closes the write side (Node `allowHalfOpen=false` behavior), so server shutdown does not wait for the graceful timeout.

## Deviations
- The TS `unix-connection` test uses a fake socket to observe queueing; Rust uses a real `UnixStream` pair with a blocked 1 MiB write, which exercises the same ordering and bound.
- Stale-socket fixture uses a dropped real listener instead of a killed child process; the on-disk socket is equally dead.
