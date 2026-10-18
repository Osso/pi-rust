# state traceability

Pinned source: `packages/client/src/state.ts`. Rust: `crates/pi-client/src/state.rs`.

| TypeScript symbol / branch | Observable contract | Rust mapping and proof |
|---|---|---|
| `ClientState.snapshot` | Last accepted server snapshot | `ClientState::snapshot`; PiClient snapshot tests |
| `reset` | Clears snapshot, session snapshots, attachments | `ClientState::reset`; invoked by `PiClient::connect` when disconnected |
| `clearAttachments` | Drops attachment markers without snapshots | `ClientState::clear_attachments`; connection state-change tests |
| `dispose` | Clears state and all listeners | `ClientState::dispose`; disposal tests |
| `getSessionSnapshot` | Snapshot lookup by id | `get_session_snapshot`; session snapshot assertions |
| `isSessionAttached` | Attachment lookup | `is_session_attached`; lease attach tests |
| `forgetSessionSnapshot` / `restoreSessionSnapshot` | Remove before reattach, restore only when absent on failure | `forget_session_snapshot`, `restore_session_snapshot`; lower-revision reacquire test and attach failure path |
| `subscribe` / `onEvent` | Global listener registration with unsubscribe | `subscribe`, `on_event`; state test |
| `subscribeSession` / `onSessionEvent` | Per-session listeners with unsubscribe and empty-map cleanup | `subscribe_session`, `on_session_event`; state unsubscribe test |
| `applyResult` list | Ignore | `apply_result` early return; request tests |
| `applyResult` detach | Remove attachment, force stored snapshot to `attached: false` | `apply_result` detach branch; detach tests |
| `applyResult` other | Store session snapshot unless revision is older | `store_session_snapshot(force=false)`; delayed-response test |
| `applyEvent` server_snapshot | Ignore lower revision; replace and notify snapshot listeners | `apply_event` server branch; metadata snapshot test |
| `applyEvent` session_snapshot | Same revision rule, then session snapshot listeners | `apply_event` session branch; event revision tests |
| `applyEvent` session_removed | Delete snapshot and attachment | `apply_event` removal branch; lease invalidation path |
| `applyEvent` session_progress | No snapshot change | `apply_event` progress branch; state test |
| global then per-session event listeners | Event id mapping for snapshot/progress/removed | `event_session_id`; event listener test |
| `applyServerSnapshot` | Ignore lower revision, notify | `apply_server_snapshot`; handshake tests |
| listener throw | Report via `onListenerError`; state unchanged | `catch_unwind` + `report_listener_error`; subscriber-failure tests |
| listener-error handler throws | Swallowed | `catch_unwind` around handler |

## Differential coverage

The scripted differential asserts the observed revision sequence and final snapshot/attachment state for event, response, and detach paths.

## Gaps / discrepancies

- JavaScript `Set` iteration observes additions/removals during notification; Rust snapshots the listener list before notifying. No source test covers that edge.
- Session-snapshot notifications are collected after the state mutation and invoked before global event listeners, matching source order.
