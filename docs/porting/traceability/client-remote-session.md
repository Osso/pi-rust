# coding-agent remote session traceability

Pinned source: `packages/coding-agent/src/client/remote-session.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.
Rust: `crates/pi-coding-agent/src/client/remote_session.rs`.
Focused tests: `crates/pi-coding-agent/tests/client_remote_session.rs` (8, ported from `test/client/remote-session.test.ts`
and exercised against an in-memory protocol server in `tests/support/remote_session_memory.rs`).

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `RemoteSessionOperation`, `RemoteSessionLifecycle`, `RemoteSessionState` | busy operation, unbound/ready/disposed, projected state | same-named enums/struct |
| `id`, `state`, `snapshot`, `phase`, `operation`, `models`, `sessions`, `connectionState`, `disposed` | accessors | same-named methods; `becomes_unbound...`, `dispose_is_idempotent...` |
| `subscribe` | immediate current-state call, unsubscribe, disposed rejection | `subscribe`; `reports_subscriber_failures_without_interrupting_other_subscribers` |
| `onConnectionStateChange` | delegate to client | `on_connection_state_change` |
| `RemoteSession.open` / `create` | attach/create exclusive lease; dispose on failure | `open`, `create`, `open_session`, `create_session`; `open_session` helper in every test |
| `submit` | trim, empty no-op, idle→prompt, turn→steer, phase error text | `submit`; `exposes_the_active_operation_while_prompting`, `steers_when_the_server_session_is_in_a_turn` |
| `abort` | preempts a pending submit; idle no-op | `abort`; `aborts_while_a_prompt_response_is_pending` |
| `setModel`, `setThinking` | idle-only operations with exact error text | `set_model`, `set_thinking`; `rejects_conflicting_operations_while_locally_busy` |
| `reconnect` | reconnect then re-acquire and bind | `reconnect` |
| `dispose` | idempotent, clears listeners/subscriptions, aggregates cleanup errors | `dispose`; `dispose_is_idempotent_and_marks_the_session_disposed` |
| `#replace` / `#prepareReplacement` | attach while idle; detach previous; missing-snapshot error | `replace`, `prepare_replacement`; covered through `open_session` |
| `#handleEvent` | `session_removed` unbinds; `session_progress` updates projection | `handle_event`; `becomes_unbound_and_can_reopen_after_its_session_is_removed`, progress test |
| `settleRemoteSessionDisposal` | one error propagates, several become an aggregate | `RemoteSessionError::Aggregate` |

## Gaps

- `dispose` waits for in-flight attachment operations through a counter/`Notify` instead of a promise set; the
  observable ordering (dispose returns after attachment work settles) is preserved.
- The dispose race drops the running future (`tokio::select!`) after cancelling the signal; TypeScript leaves the
  promise pending but performs the same guarded state restore.
- Listener failures surface as the panic payload (`panic!("render failed")` → `"render failed"`); TypeScript also
  accepts thrown non-`Error` values, which Rust listeners cannot express.

## Discrepancies

None.
