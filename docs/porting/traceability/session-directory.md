# coding-agent session-directory traceability

Pinned source: `packages/coding-agent/src/core/session-directory.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Status: ported. Rust module: `crates/pi-coding-agent/src/core/session_directory.rs`.
Focused tests: `crates/pi-coding-agent/tests/session_directory.rs` (9).

| TypeScript source | Required control-db symbols | Status |
|---|---|---|
| `SessionDirectoryOptions`, `BroadcastSessionFilters`, `BroadcastSessionsInput` | none (types) | `SessionDirectoryOptions`, `BroadcastSessionFilters`, `BroadcastSessionsInput` |
| `reconcileCurrentMainSessionBindings` (private) | `list_runtime_mailbox_listeners`, `retire_runtime_mailbox_listener`, `is_pi_runtime_process_alive` | `reconcile_current_main_session_bindings`; `expires_a_fresh_binding_when_its_pid_is_not_a_pi_runtime`, `uses_the_newest_main_session_listener_for_a_shared_live_pid` |
| `listSessions` | `list_active_session_metadata`, `list_session_health`, `write_session_health`, `list_runtime_mailbox_listeners`, `retire_runtime_mailbox_listener` | `list_sessions`; all 9 tests |
| `broadcastToSessions` | `list_runtime_mailbox_listeners`, `upsert_multi_agent_mailbox_message`, `enqueue_runtime_mailbox_message`, plus `session_health::{derive_live_status,is_sticky_dead,is_session_eligible_to_receive,parse_goal_objective}` | `broadcast_to_sessions`; `broadcasts_only_to_eligible_matching_current_sessions`, `broadcast_requires_a_non_empty_message` |

Pure helpers (`newestMetadataBySessionId`, `bindingHeartbeatIsFresh`, `synchronizedBoundSessionHealth`, `toDirectoryEntry`, `matchesFilters`) are private and ported as same-named snake_case functions.

## Gaps

- The `now` option takes an injected epoch-millisecond clock instead of `() => Date`; behavior is identical.
- Broadcast ordering follows metadata order (`modified_at DESC, updated_at DESC, session_path DESC`); the source test asserts membership, not order.
