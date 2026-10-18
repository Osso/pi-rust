# connection traceability

Pinned source: `packages/client/src/connection.ts`. Rust: `crates/pi-client/src/connection.rs`.

| TypeScript symbol / branch | Observable contract | Rust mapping and proof |
|---|---|---|
| `Connection` construction | Reject `maxFrameLength` outside `1..=2^32-1` | Validation moved to `PiClient::new` (`u32` excludes overflow); `connection::rejects_frame_limits_outside_unsigned_32_bit_range` |
| `state`, `maxFrameLength` | Read-only lifecycle and limit | `Connection::{state, max_frame_length}` |
| `connect()` while not disconnected | Reject `PiClient is already <state>` | `Connection::connect` returns `PiError::Disconnected` with that message; covered indirectly by reconnect tests |
| `connect()` | Emit `connecting`, create decoder/handshake, open transport in background, await server hello | `Connection::connect` + spawned `open_transport`; `sends_framed_version_before_accepting_fragmented_hello` |
| server data before client hello | Fail with `Received server data before the client hello was sent`, close once | connecting lifecycle has no transport → `fail_and_close`; `rejects_server_data_before_client_hello_is_sent` asserts send/close counts |
| factory rejection | Convert to disconnected error and fail handshake | `open_transport`; reconnect/factory tests |
| transport resolves after state changed | Close the fresh transport | `accepted` check in `open_transport` |
| hello send failure | Fail and close when still current | `open_transport` send error branch |
| fragmented/coalesced frames | Decoder buffers arbitrary chunks | `handle_data`; fragmented hello and `sendTogether` request test |
| decoder failure | Fail and close with `ProtocolValidationError` | `handle_data` error branch; invalid-data and frame-limit tests |
| `hello_error` first | Reject with `PiServerError` | `handle_message`; `rejects_typed_handshake_version_error` |
| non-hello first message | Reject `Expected server hello as first message` | `handle_message`; no dedicated source test (branch mapped) |
| hello before client hello sent | Reject `Received server hello before the client hello was sent` | `handle_message`; branch mapped |
| hello transition | Set connected, `onHandshake`, `onStateChange`, resolve handshake, abort if lifecycle changed | `PrepareHandshake` path with identity checks; listener disconnect/reconnect tests |
| handshake/hello messages after connected | Reject `Unexpected handshake message` | `handle_message` connected branch; branch mapped |
| response/event after connected | Forward to `onMessage` | `Action::Notify`; all request/session tests |
| transport close | `decoder.end()` first; decoder error wins over disconnected error | `handle_close`; `reports_truncated_framing_when_transport_closes` |
| `fail` | Set disconnected, reject handshake, emit state change | `fail_with(error, false)` |
| `failAndClose` | Fail first, then close transport | `fail_with(error, true)`; close-count assertions |
| `send` while not connected | Throw disconnected error | `Connection::send` returns `Err`; PiClient checks first |
| async send rejection | Fail only when the same transport is still current | spawned send watcher with `Arc::ptr_eq`; transport-error test |
| disconnect default reason | `"Client disconnected"` | `PiClient::disconnect` |

## Differential coverage

`differential_client::differential_scripted_client_session` drives the TypeScript client and Rust client through the same scripted in-memory server and compares hello/list/attach/prompt/detach frame hex, connection-state sequence, snapshot revisions, and final state.

## Gaps / discrepancies

- Source `Connection` is not exported from the package index; Rust keeps it private.
- Callbacks are invoked outside the lifecycle mutex, with explicit identity re-checks, preserving the source's reentrancy behavior without holding a lock across user code.
