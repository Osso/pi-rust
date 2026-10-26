# coding-agent request-wake traceability

Pinned source: `packages/coding-agent/src/supervisor/request-wake.ts` at `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d` (read-only).

Proof: behavioral tests in `crates/pi-coding-agent/tests/request_wake.rs` (`suite_core_4`), using real Unix sockets, stale-socket recovery and wake pings. The wire contract is a connection with no payload, followed by EOF; native tests exercise both notify-to-listener and client-to-listener directions. Frozen/native lifecycle observations are recorded in `/tmp/reaudit-db-ipc/wake-owner-{ts,rust}.log`, with probes `wake-probe.mts` and `probe/src/bin/wake.rs`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `getSupervisorRequestSocketPath` | `<controlDb>.supervisor.sock` | `get_supervisor_request_socket_path`; `derives_the_socket_path_from_the_control_database` |
| `notifySupervisorRequest` | Fire-and-forget connect, errors ignored, unref'd | `notify_supervisor_request` (blocking `UnixStream::connect`, dropped); `notifying_a_missing_socket_is_silent` |
| `SupervisorRequestWakeServer.start` | Windows rejected; parent dir `0700`; stale-socket recovery; socket `0600`; idempotent | `start`, `create_private_directory`, `set_private_socket_mode`; `recovers_a_stale_socket_file`, `start_is_idempotent` |
| `currentGeneration` | Increments per accepted connection, survives close/restart | `current_generation` over `watch`; `wakes_waiters_and_advances_the_generation_on_each_request`, restart regression |
| `waitForWakeAfter` | Resolves on abort, close or generation advance, not an unseen historical notification | `wait_for_wake_after` marks the current watch version seen and rechecks generation on each change; `historical_wake_does_not_resolve_a_wait_for_the_current_generation`, `returns_immediately_for_already_observed_generations_and_aborts` |
| `close` | Resolves waiters, destroys clients, closes the server, removes the socket; later start works | `close`, fresh shutdown controller on restart; `close_resolves_pending_waiters`, `close_then_start_reopens_the_listener_without_resetting_generation` |
| Stale-socket recovery | `EADDRINUSE` + accepting socket → `Supervisor request socket is already active: <path>`; otherwise remove and rebind | `listen_with_stale_socket_recovery`; `refuses_to_replace_an_active_socket`, stale test |
| Rejected listener ownership | Failed start does not register exit cleanup or remove another listener's socket on object disposal | `Drop` removes the socket only when the object owns an accept task; `dropping_a_rejected_candidate_preserves_the_active_owners_socket` |
| `assertUnixSocketSupport` | Windows → `Supervisor request wake requires Unix domain sockets` | `assert_unix_socket_support`; Windows execution is an approved gap |

## Native boundaries

Control-DB/socket paths and errors remain native `JsString`; binding, connection and private filesystem helpers receive Node UTF-8 only at their OS boundary. `tests/lossless_supervisor_wake.rs` covers raw path derivation and a real wake ping.

## Exclusions

- The source registers a `process.once("exit")` cleanup hook only after successful listen; Rust removes an owned socket in `Drop` and removes the socket on explicit `close`.
- Accepted connections are dropped immediately (the source tracks them only to destroy on close; nothing survives to be destroyed).
- OS/runtime fault shapes, pathological inputs and scheduling races remain under the repository parity exclusions; these regressions use normal sequential lifecycle operations.

## Discrepancies resolved (F02–F04)

- F02: after accepting generation 1, waiting after 1 stays pending until another ping. The regression directly polls the future, then uses a real second connection to prove progress.
- F03: start → accepted generation 1 → close → start accepts generation 2. A further EOF-synchronized ping proves the restarted loop stays active (generation 3).
- F04: rejected candidate start → candidate drop preserves the active owner's pathname and reachability. The regression checks the exact frozen active-socket error before disposal.

Each regression failed on `ededb0e77` before production edits; `/tmp/request-wake-red.log` records all three expected failures. No frozen source or wire format changed.
