# coding-agent session-directory traceability

Pinned source: `packages/coding-agent/src/core/session-directory.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Status: **blocked** on the parallel `ca-control-db` slice. Every exported function reads or writes the control database; there is no Rust module yet.

| TypeScript source | Required control-db symbols | Status |
|---|---|---|
| `SessionDirectoryOptions`, `BroadcastSessionFilters`, `BroadcastSessionsInput` | none (types) | ready to port with the module |
| `reconcileSessionRuntimeBindings` | `list_runtime_mailbox_listeners`, `retire_runtime_mailbox_listener`, `is_pi_runtime_process_alive` | blocked |
| `listSessions` | `list_active_session_metadata`, `list_session_health`, `write_session_health`, `list_runtime_mailbox_listeners`, `retire_runtime_mailbox_listener` | blocked |
| `broadcastToSessions` | `list_runtime_mailbox_listeners`, `upsert_multi_agent_mailbox_message`, `enqueue_runtime_mailbox_message`, plus `session_health::{derive_live_status,is_sticky_dead,is_session_eligible_to_receive,parse_goal_objective}` (ported) | blocked |

Pure helpers (`newestMetadataBySessionId`, `bindingHeartbeatIsFresh`, `synchronizedBoundSessionHealth`, `toDirectoryEntry`, `matchesFilters`) are private in the source and will be ported with the same commit that wires the control-db API.

The parent session confirmed: `ca-control-db` will export exactly these names from `pi_coding_agent::core::session_control_db`. After it merges, this module is implemented and verified against `packages/coding-agent/test/session-directory.test.ts`.

## Gaps

- Whole module blocked; `docs/porting/coverage.md` row is `partial` with no Rust path.
