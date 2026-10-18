# coding-agent request-wake traceability

Pinned source: `packages/coding-agent/src/supervisor/request-wake.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Proof: behavioral socket tests in `crates/pi-coding-agent/tests/request_wake.rs` (real Unix sockets, real stale-socket recovery, real wake pings). Differential testing is not applicable: the module owns a live listener.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `getSupervisorRequestSocketPath` | `<controlDb>.supervisor.sock` | `get_supervisor_request_socket_path`; `derives_the_socket_path_from_the_control_database` |
| `notifySupervisorRequest` | Fire-and-forget connect, errors ignored, unref'd | `notify_supervisor_request` (blocking `UnixStream::connect`, dropped); `notifying_a_missing_socket_is_silent` |
| `SupervisorRequestWakeServer.start` | Windows rejected; parent dir `0700`; stale-socket recovery; socket `0600`; idempotent | `start`, `create_private_directory`, `set_private_socket_mode`; `recovers_a_stale_socket_file`, `start_is_idempotent` |
| `currentGeneration` | Increments per accepted connection | `current_generation` over `watch`; wake test asserts 0 → 1 |
| `waitForWakeAfter` | Resolves when aborted or the generation advanced; re-checks after registering | `wait_for_wake_after`; `wakes_waiters_and_advances_the_generation_on_each_request`, `returns_immediately_for_already_observed_generations_and_aborts` |
| `close` | Resolves waiters, destroys clients, closes the server, removes the socket | `close` (shutdown token, aborted accept task, socket removal); `close_resolves_pending_waiters` |
| Stale-socket recovery | `EADDRINUSE` + accepting socket → `Supervisor request socket is already active: <path>`; otherwise remove and rebind | `listen_with_stale_socket_recovery`; `refuses_to_replace_an_active_socket`, stale test |
| `assertUnixSocketSupport` | Windows → `Supervisor request wake requires Unix domain sockets` | `assert_unix_socket_support` (cfg-gated; not reachable on Linux tests) |

## Exclusions

- The source registers a `process.once("exit")` cleanup hook; Rust removes the socket in `Drop` and `close`, which covers the same lifetime.
- Accepted connections are dropped immediately (the source tracks them only to destroy on close; nothing survives to be destroyed).

## Discrepancies

None.
