# coding-agent resident-console-transport traceability

Pinned source: `packages/coding-agent/src/core/resident-console-transport.ts` at `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`. Whole-module re-audit completed 2026-10-25 under the lighter-proof rule.

Proof:

- Behavioral tests in `crates/pi-coding-agent/tests/resident_console_transport.rs` (13 cases) covering attach/snapshot, prompt + monotonic events, ownership transfer, missing socket, version/service mismatch, missing attach, invalid messages, prompt rejection, duplicate/empty prompt ids, closed clients, active/stale sockets, and protocol constants.
- Cross-language wire compatibility in `crates/pi-coding-agent/tests/differential_resident_console.rs` (requests `27020`–`27024`): the pinned TypeScript client attaches, prompts, and receives Rust-server events; the Rust client attaches, prompts, and receives TypeScript-server events.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `RESIDENT_CONSOLE_PROTOCOL_VERSION` | `1` | `RESIDENT_CONSOLE_PROTOCOL_VERSION` |
| `ResidentConsoleService` | `architect \| supervisor` | `enum ResidentConsoleService` |
| `ResidentConsoleSnapshot<Entry>` | camelCase `service/sessionId/cwd/generation/branch` | `ResidentConsoleSnapshot<Entry>` |
| `ResidentConsoleServerOptions` | socket path, service, `getSnapshot`, `enqueuePrompt`, `subscribe` | `ResidentConsoleServerOptions` + `ResidentConsoleHost` trait (`HostFuture`, `Unsubscribe`) |
| `start` | Windows rejected; parent dir `0700`; stale-socket recovery; socket `0600`; idempotent | `start`; active/stale tests |
| `close` | Releases owner, destroys clients, closes server, removes socket | `close`; owner-transfer test |
| `acceptSocket` | Line-delimited JSON; blank lines ignored; invalid JSON → `invalid_message` | `serve_connection`/`handle_line`; invalid-message test |
| `attach` | Version mismatch → `version_mismatch`; service mismatch → `service_mismatch`; newest client becomes owner (previous ends) | `attach`; mismatch and ownership tests |
| `handleMessage` | Non-owner → `missing_attach`; `disconnect` ends; `prompt` requires non-empty `id` and `text` | `handle_owned_message`; missing-attach/invalid-message tests |
| `enqueuePrompt` | `prompt_accepted {id}` or `error {code:"prompt_rejected", message, id}` | `enqueue_prompt`; rejection test |
| Event fan-out | `event {sequence, event}` with a monotonically increasing sequence per server | `ServerState.sequence` under a mutex, delivered through an ordered per-connection channel; event test and differential `27020` |
| `ResidentConsoleClient.connect` | Missing/refused socket → `Resident console is unavailable: <path>`; error response → its message or `Resident console request failed`; shape mismatch → `Invalid resident console response` | `connect`, `parse_attach_response`, `connect_resident_console`; missing-socket test, differential `27021` |
| `prompt` | Closed → `Resident console is closed`; empty id/text → `Resident console prompt id and text are required`; duplicate → `Resident console prompt already pending: <id>` | `prompt`; rejection test |
| `onEvent`/`onDisconnect` | Listener registration returning an unsubscribe function | `on_event`/`on_disconnect`; event and disconnect tests |
| `close` | Writes `disconnect`, ends the socket, waits for close | `close` (watch-based finish); every focused test closes cleanly |
| `handleMessage` (client) | Events dispatched to listeners; `prompt_accepted`/`error` settle the pending prompt; unknown ids ignored | `handle_client_line` |
| `finish` | Rejects pending prompts with the error or `Resident console disconnected`, notifies close listeners once | `ClientState::finish` with a finish guard |

## Native compiled identity helper (2026-10-25)

`ResidentConsoleIdentity::for_compiled_process` supplies the current executable as both executable and entrypoint, per the parent-approved compiled-runtime mapping; argv[1] is not a script path. Supervisor construction and real flag-child proof: [Supervisor main](supervisor-main.md). This helper does not establish the transport module's common-pin re-audit.

## Exclusions

- `Entry`/`Event` are generic `Serialize + DeserializeOwned` types instead of TypeScript generics; the wire shape is unchanged.
- `enqueuePrompt` returns a boxed future (`HostFuture`) instead of `Promise<void> | void`; hosts that complete synchronously return a ready future.
- Listener callbacks take `&ResidentConsoleEvent<Event>` instead of an owned value.

## Common-pin audit and native regressions

Audited all 511 frozen lines: server start/close and socket recovery, line dispatch, attach/probe validation and owner transfer, prompt completion, subscriptions/sequence, client connect/snapshot/identity, pending prompt settlement and disconnect. `resident_console_drift` retains common-pin identity and non-displacing probe proof. Exact UTF-16 snapshots, events, prompt ids/text and rejection messages remain covered by `lossless_resident_console`.

- Fixed snapshot/event ordering: enqueue `attached` before invoking a synchronously emitting subscription. Native RED→GREEN: `sends_snapshot_before_events_emitted_during_subscription`.
- Fixed remote-close state: `finish` marks the client closed before notifying listeners, so later prompts reject instead of hanging. Native RED→GREEN: `rejects_new_prompts_after_remote_disconnect`.
- Fixed EOF before initial snapshot: use `Resident console disconnected before attach`. Native RED→GREEN: `reports_disconnect_before_snapshot`.
- Fixed an existing regular file at the parent-directory path: fail the mkdir operation with the frozen EEXIST message, not a later raw socket ENOTDIR. Native RED→GREEN: `reports_node_errors_for_existing_file_parent_and_connecting_to_a_directory`.

Proof logs `/tmp/runtime-successor-{transport-final,transport-wire,transport-lossless}.log`: Core4 resident group 25 PASS (13 transport, 2 drift, 10 command); two reciprocal frozen client/server tests PASS; two exact-UTF-16 transport tests PASS. No new differential fixture or matrix.

## Gaps / discrepancies

- Malformed/adversarial wire shapes, numeric extremes, native callback panics, exotic socket/fd/OS failures and Node chunk/microtask scheduling oddities remain the approved runtime/input gap; existing lossless carriers stay intact.
- DISCREPANCY (preserved frozen behavior): a valid `probed` snapshot can satisfy an attach request, and a valid `attached` snapshot can satisfy a probe request; the parser accepts both tags without correlating the request kind.
