# server/server.ts traceability

Source: `packages/server/src/server.ts`. Rust: `crates/pi-server/src/server.rs`.

| TS item | Rust | Proof |
|---|---|---|
| `PiServer` | `PiServer` + `ServerCore` | `tests/server.rs`, `tests/conformance.rs` |
| `id`/`addresses` | `id`/`addresses` | listener and unix tests |
| `start`/`startInternal` | `start` (sets state synchronously, returns boxed future) + `start_internal` | concurrent-start, listener-failure tests |
| `accept` | `accept` | handshake timeout and unix tests |
| `close`/`closeInternal` | `close`/`close_server_state` | graceful close conformance test |
| `receive`/`dispatchMessage` | `receive`/`dispatch_message` | malformed/oversized frames, fragmented hello, out-of-order responses |
| `finishHandshake` | `finish_handshake` | version/first-message/duplicate/timeout/catch-up tests |
| `handleRequest` | `handle_request` | all request tests |
| `transportClosed`/`disconnect` | `transport_closed`/`disconnect` | disconnect/terminal-error tests |
| `sendMessage`/`failProtocol` | `send_message`/`fail_protocol` | blocked-output timeout test, malformed frames |
| `toProtocolError`/`reportError` | same names | internal/not-implemented/wrapped-cause tests |

## Branches
- Invalid options: `PiServerConfigError` (empty server id, zero frame length, zero/overflow handshake timeout); covered by `tests/server.rs` and `tests/unix.rs`.
- `listeners` is a typed `Vec`, so the TS “listeners must be an array” failure is unrepresentable; empty lists are accepted (`empty_listener_list_is_valid_and_has_no_addresses`).
- Handshake timeout remains armed until the server hello is sent and is cancelled on success/failure (`closes_connections_that_do_not_complete_hello_before_the_timeout`, `keeps_the_handshake_timeout_active...`).
- Out-of-order responses: requests are spawned as tasks (`can_respond_out_of_request_order_after_the_handshake`).
- Error observers are isolated with `catch_unwind`; observer panics do not change server state (`does_not_expose_unexpected_service_errors_to_clients`).

## Deviations
- Connection/server ids use `pi_ai::utils::uuid::uuidv7` instead of Node `randomUUID` (v4). Both are collision-resistant; differential tests normalize UUID strings.
- `start()` performs validation and claims the `starting` flag synchronously before returning its future, mirroring the TS `startPromise` assignment.
