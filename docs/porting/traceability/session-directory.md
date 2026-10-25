# coding-agent session-directory traceability

Pinned source: `packages/coding-agent/src/core/session-directory.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `crates/pi-coding-agent-core/src/core/session_directory.rs`.

| TypeScript source | Rust item | Tests |
|---|---|---|
| `SessionDirectoryOptions` | `SessionDirectoryOptions` (`now_ms`, `include_ended`, `is_runtime_process_alive`) | `tests/session_directory.rs`; differential scenarios |
| `BroadcastSessionFilters` | `BroadcastSessionFilters` | broadcast tests + differential |
| `BroadcastSessionsInput` | `BroadcastSessionsInput` | broadcast tests + differential |
| `reconcileSessionRuntimeBindings` | `reconcile_session_runtime_bindings` | `retires_stale_listeners_while_listing`, differential |
| `listSessions` | `list_sessions` | 12 behavioral tests; `session_directory_list_differential` (6 scenarios) |
| `broadcastToSessions` | `broadcast_to_sessions` | 5 behavioral tests; `session_directory_broadcast_differential` (3 scenarios) |
| private `resolveNow`, `healthBySessionId`, `newestMetadataBySessionId` | same private helpers | exercised through both public functions |
| private `reconcileCurrentMainSessionBindings` (listener sort by `updatedAt` desc then `sessionId` localeCompare) | `reconcile_current_main_session_bindings` (`pi_js_compat::collator::locale_compare`) | duplicate-pid and duplicate-metadata differential scenarios |
| private `bindingHeartbeatIsFresh` | `binding_heartbeat_is_fresh` (`pi_js_compat::date::date_parse`) | stale/future heartbeat behavioral tests + differential |
| private `synchronizedBoundSessionHealth`, `syncBoundSessionHealth` | same names | generation advance, timeout status |
| private `retireUnboundSessionHealthIfNeeded` | same | unbound historical pid test + differential |
| private `toDirectoryEntry` | `to_directory_entry` | list differential |
| private `matchesFilters` | `matches_filters` | broadcast filter behavioral test + differential |
| `broadcast_<uuid>` message id and canonical mailbox payload key order (`body`, `fromAgentId`, `id`, `kind`, `status`, `toAgentId`) | `send_broadcast`, `mailbox_payload` (`pi_ai::utils::uuid::random_uuid`, `Map` insertion order) | broadcast differential (payload compared through mailbox rows) |

Notes:

- Runtime liveness defaults to `runtime_process::is_pi_runtime_process_alive`; tests inject a probe.
- `now` injection is `now_ms: Option<i64>`; `Date` formatting uses `utils::iso_time::format_iso_millis`.
- Differential: `oracle/coding-agent-session-tools-oracle.ts` operations
  `session-directory:list` and `session-directory:broadcast`; each side sets up its own
  control DB from identical setup ops, then compares sessions/results/listeners/health/mailbox rows.
- Multi-agent lifecycle rows are never read or written here, so the source tests asserting
  “does not rewrite historical duplicate lifecycle rows” hold trivially (no call sites).

## Gaps

- `readArchitectSnapshot` side effects in the source test are not ported: the architect
  observer module is outside this slice. `listSessions` itself performs the same health
  retirement that the snapshot relies on.
