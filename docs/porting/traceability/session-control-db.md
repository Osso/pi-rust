# `coding-agent/src/core/session-control-db.ts`

Source: `packages/coding-agent/src/core/session-control-db.ts` (7047 lines) at the pinned revision.
Rust: `crates/pi-coding-agent/src/core/session_control_db/` (`mod.rs`, `schema.rs`, `sessions.rs`,
`incoming.rs`, `mailbox.rs`, `requests.rs`, `channel.rs`, `health.rs`, `agents.rs`, `lifecycle.rs`).

The shared `control.sqlite` is opened by TypeScript and Rust processes at the same time. SQL text,
schema version, migrations, PRAGMAs, and transaction boundaries are kept verbatim.

## Schema and connection

| Contract | Rust | Proof |
|---|---|---|
| `CONTROL_DB_SCHEMA_VERSION = 14` | `CONTROL_DB_SCHEMA_VERSION` | `schema_and_pragmas_match_the_shared_contract`, `legacy_database_migrates_to_schema_fourteen`, `unsupported_future_schema_is_rejected` |
| `CREATE TABLE`/`CREATE INDEX` for all 17 tables | `schema.rs::CREATE_SCHEMA_SQL` | `cross_implementation_control_db::schema_text_and_pragmas_match_the_typescript_module` |
| Migration from legacy counters, payloads, runtime owners, outbox, mailbox, lifecycle rows, architect/listener/session-metadata columns | `initialize_schema` + `migrate_*` | `legacy_database_migrates_to_schema_fourteen` |
| Legacy artifact-field rejection triggers | `create_legacy_artifact_field_triggers` | migration test; trigger names asserted by cross-language schema test |
| `journal_mode=WAL`, `synchronous=NORMAL`, `busy_timeout=5000` | `configure_shared_sqlite_database` | `schema_and_pragmas_match_the_shared_contract`, cross-language schema test |
| `getControlDbPath(directory = getUserStateRoot())` | `get_control_db_path(&Path)` | `control_db_path_joins_control_sqlite` |
| `getRuntimeProcessInstanceId()` | `get_runtime_process_instance_id()` | listener registration/claim tests |
| `retainControlDbConnection()` | `retain_control_db_connection()` + `RetainedControlDbGuard` | `retained_connection_survives_and_releases` |

Every write runs inside `BEGIN IMMEDIATE` (`with_immediate_transaction`), matching TS; reads use a
short-lived or retained connection. `ControlDbError` carries the SQLite contention flag used by the
TS `isSqliteContentionError` retry branches.

## Exported symbols by area

All names below are re-exported from `pi_coding_agent::core::session_control_db` under the
snake_case form of the TypeScript name; the area's test file is `tests/session_control_db.rs`
unless noted.

| Area | TypeScript exports | Rust functions | Tests |
|---|---|---|---|
| Incoming messages | `enqueueIncomingMessage`, `claimLatestIncomingMessage`, `completeIncomingMessage`, `failIncomingMessage`, `readIncomingMessageStatus`, `IncomingControlMessage` | `incoming.rs` | `incoming_claim_returns_none_when_queue_is_empty`, `incoming_messages_claim_supersedes_pending_rows` |
| Runtime mailbox | `enqueueRuntimeMailboxMessage`, `enqueueStoredRuntimeMailboxMessage`, `recoverDeadRuntimeMailboxClaims`, `takeRuntimeMailboxMessagesForDelivery`, `claimRuntimeMailboxMessages`, `readRuntimeMailboxMessageForDelivery`, `consumeRuntimeMailboxMessage`, `deliverRuntimeMailboxMessage`, `consumeRuntimeMailboxMessageByStoreRef`, `markRuntimeMailboxMessageDelivered`, `releaseRuntimeMailboxMessageClaim`, `failRuntimeMailboxMessage`, `readRuntimeMailboxMessage`, `listRuntimeMailboxMessages` | `mailbox.rs` | `runtime_mailbox_routing_preserves_payload_order_and_identity`, `runtime_mailbox_claim_respects_registered_listener_pid`, `runtime_mailbox_claim_deliver_and_consume`, `mailbox_status_transitions_require_pending_state` |
| Listener registration | `registerRuntimeMailboxListener`, `retireRuntimeMailboxListener`, `readRuntimeMailboxListener`, `listRuntimeMailboxListeners`, `assertMainSessionRuntimeAvailable`, `resolveOwnMainRuntimeCoordinationRecipient`, `RuntimeMailboxListener`, `RuntimeMailboxRegistrationOptions`, `RuntimeMailboxAddress`, `RuntimeMailboxStoreRef` | `mailbox.rs` | `session_health_and_listener_registration_track_generations`, `runtime_mailbox_claim_respects_registered_listener_pid` |
| Architect requests | `postArchitectRequest`, `listPendingArchitectRequests`, `claimPendingArchitectRequests`, `renewArchitectRequestClaims`, `completeArchitectRequest`, `ArchitectRequest*` | `requests.rs` | `architect_requests_claim_renew_and_complete` |
| Supervisor requests | `postSupervisorRequest`, `readSupervisorRequest`, `claimNextSupervisorRequest`, `recoverSupervisorRequests`, `hasPendingSupervisorApprovalRequest`, `requeueSupervisorRequest`, `completeSupervisorRequest`, `Supervisor*` | `requests.rs` | `supervisor_requests_prioritize_approvals_and_validate_responses`, `supervisor_recovery_completes_expired_requests`, `supervisor_advisory_claim_requires_matching_response_kind` |
| Shared channel | `postSharedChannelMessage`, `listSharedChannelMessagesAfter`, `readSharedChannelCursor`, `hasPendingRuntimeCoordinationMessage`, `isRuntimeCoordinationMailboxMessage`, `initializeSharedChannelCursorAtTail`, `advanceSharedChannelCursor`, `readSharedChannelTail`, `SharedChannelMessage`, `PostSharedChannelMessageInput` | `channel.rs` | `shared_channel_posts_and_cursors`; differential `empty_channel_body_error_matches` |
| Session health | `readSessionHealth`, `listSessionHealth`, `writeSessionHealth` | `health.rs` | `health_record_round_trips_all_statuses` |
| Session metadata | `writeSessionMetadata`, `readSessionMetadata`, `listSessionMetadata`, `listActiveSessionMetadata`, `listArchivedSessionMetadata`, `findActiveSessionMetadataById/Name`, `removeSessionMetadata`, `archiveSession`, `unarchiveSession`, `archiveSessionsOlderThan`, `SessionMetadata`, `WritableSessionMetadata` | `sessions.rs` | `session_metadata_round_trips_and_archives`, `session_metadata_write_preserves_archived_state_and_index_flag` |
| Named sessions / relocation | `setNamedSession`, `removeNamedSession`, `listNamedSessions`, `relocateSessionControlData`, `NamedSession` | `sessions.rs` | `named_sessions_and_relocation_move_all_control_rows` |
| Last message / prompt history | `writeLastMessage`, `readLastMessage`, `recordPromptHistoryEntry`, `readPromptHistory`, `readOrMigratePromptHistory`, `LastControlMessage` | `sessions.rs` | `last_message_only_returns_assistant_roles`, `prompt_history_deduplicates_and_migrates_legacy_entries` |
| Goal / model / thinking / sandbox | `readSessionGoal`, `writeSessionGoal`, `writeSessionModel`, `writeSessionThinkingLevel`, `readSessionSandboxProfile`, `writeSessionSandboxProfile`, `clearSessionSandboxProfile` | `sessions.rs` | `session_metadata_identity_is_enforced_for_sandbox_profiles` |
| Multi-agent persistence | `bootstrapMultiAgentAgent`, `readMultiAgentAgent(s)`, `readMultiAgentState`, `upsertMultiAgentMailboxMessage`, `getMultiAgentMailboxMessageStatus`, `markMultiAgentMailboxMessageDelivered/Failed`, `writeMultiAgentCounters`, `allocateMultiAgentCounter`, `updateMultiAgentAgentActivity`, `updateMultiAgentAgentCurrentActivity`, `updateMultiAgentAgentSlot`, `updateMultiAgentAgentTranscript`, `MultiAgent*` records | `agents.rs` | `read_multi_agent_state_orders_rows_by_rowid`, `multi_agent_counters_allocate_monotonic_sequences`, `bootstrap_refuses_to_mutate_owned_rows` |
| Lifecycle mutations | `createMultiAgentChildWithRuntimeOwnership`, `createMultiAgentAttachment`, `createFailedMultiAgentChild`, `acquireAttachedRuntimeOwnership`, `readMultiAgentRuntimeOwnership`, `commitMultiAgentLifecycleMutation`, `commitMultiAgentSteeringMutation`, `commitMultiAgentSteeringDelivery`, `commitMultiAgentDetachMark`, `commitMultiAgentTerminalMutation`, `recoverDeadMultiAgentRuntime`, `reconcileDeadDetachedAgentRuntimes`, `MultiAgentRuntimeOwnership*`, `MultiAgentRuntimeOwnerIdentity`, `SupervisorRuntimeOwnership` | `agents.rs` | `child_creation_requires_active_parent_and_running_revision_one`, `detach_mark_is_idempotent_and_fenced_by_ownership`, `steering_mutations_require_registered_authority`, `lifecycle_and_terminal_mutations_write_the_outbox`, `agent_error_type_is_persisted_through_terminal_mutation` |
| Terminal outbox | `claimMultiAgentTerminalOutbox`, `deliverMultiAgentTerminalOutbox`, `failMultiAgentTerminalOutbox`, `cleanupMultiAgentTerminalOutbox`, `MultiAgentTerminalOutboxRecord`, `ClaimMultiAgentTerminalOutboxOptions` | `agents.rs` | `outbox_claim_recovers_stale_claims_and_poisons_after_max_attempts`, `terminal_outbox_replay_is_idempotent` |
| Detached artifacts / finalization | `listDetachedArtifactAgentsUpdatedAtOrBefore`, `finalizeDetachedJob`, `prepareControlDbForSelfRestart`, `FinalizeDetachedJob*`, `DetachedArtifactAgentRecord` | `agents.rs`, `lifecycle.rs` | `detached_artifact_listing_filters_by_label_and_lifecycle`, `finalize_detached_job_records_terminal_transport_and_artifacts` |

The session-manager consumer contract (20 symbols) is compile-locked by
`tests/control_db_public_api.rs::session_manager_contract_is_public`.

## Cross-implementation and differential proof

`tests/cross_implementation_control_db.rs` (temp DBs only):

- `typescript_writes_are_readable_by_rust`
- `rust_writes_are_readable_by_typescript_in_source_order`
- `concurrent_typescript_and_rust_writers_keep_every_row`
- `schema_text_and_pragmas_match_the_typescript_module`

`tests/differential_control_db.rs` runs 74 operation steps against
`oracle/control-db-oracle.ts` on separate temp databases and compares `JSON.stringify` output
(key order included) after masking timestamps. Covers schema creation, session metadata, named
sessions, prompt history, incoming messages, mailbox routing/claims, architect/supervisor queues,
shared channel cursors, health, multi-agent bootstrap/state/lifecycle/outbox/counters, and
relocation.

## Discrepancies / gaps

- TypeScript's dual driver (`bun:sqlite` / `node:sqlite`) collapses to `rusqlite::Connection`; the
  observable schema, SQL, and PRAGMA contract is unchanged.
- `getRuntimeProcessInstanceId`: TS throws when `/proc/self/stat` cannot be read. Rust returns
  `Err(ControlDbError)` with the identity-read message instead of synthesizing a pid/zero-start-time
  identity; the value is memoized once, matching the `globalThis` cache. Proof:
  `process_instance_id_propagates_identity_read_failure`,
  `process_instance_id_serializes_identity_in_typescript_key_order`,
  `runtime_process_instance_id_uses_the_live_process_identity`.
- Typed-core exclusion: TS accepts arbitrary runtime payloads for `enqueueStoredRuntimeMailboxMessage`
  and mailbox JSON; Rust requires typed structs. Invalid JS payload shapes cannot be constructed.

## Mailbox payload key order

Every canonical mailbox payload mutation spreads the stored object and then appends keys in the
TypeScript literal order (`claimedAt`, `claimantProcessIdentity`, `status`, `updatedAt`; routing
appends `recipientSessionId`, `recipientAgentId`, `senderSessionId`, `senderAgentId`; delivery appends
`deliveredAt` then `error`). Rust inserts in the same order with `shift_remove`/`remove` after the
inserts, so `JSON.stringify` bytes match. Covered by the 74-step `differential_control_db` suite.
