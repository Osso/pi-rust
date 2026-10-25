# coding-agent resident-console-transport traceability

Pinned source: `packages/coding-agent/src/core/resident-console-transport.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Proof:

- Behavioral tests in `crates/pi-coding-agent/tests/resident_console_transport.rs` (9 cases) covering attach/snapshot, prompt + monotonic events, ownership transfer, missing socket, version/service mismatch, missing attach, invalid messages, prompt rejection, duplicate/empty prompt ids, closed clients, active/stale sockets, and protocol constants.
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
| `ResidentConsoleClient.connect` | Missing/refused socket → `Resident console is unavailable: <path>`; error response → its message or `Resident attach failed`; shape mismatch → `Invalid resident console attach response` | `connect`, `parse_attach_response`, `connect_resident_console`; missing-socket test, differential `27021` |
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

## Discrepancies

None.
