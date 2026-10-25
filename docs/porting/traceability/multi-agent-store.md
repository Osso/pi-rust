# `coding-agent/src/core/multi-agent-store.ts`

## Storage-3 mailbox JSON ordering (2026-10-25)

Actual source-produced `recordOutboundSessionMessage` exposed two real REDs: public copyMessage JSON and canonical SQLite data used native id-first order instead of their distinct frozen orders (`/tmp/storage3-store-wire-red.log`). Expanded actual-source contact/send-main/send-child/notification/steering matrix also exposes notification canonical ordering (`/tmp/storage3-store-other-wire-valid-red.log`); direct canonical rows already matched before the fix. The native public serializer now uses frozen copyMessage order; private put sites encode direct/outbound/lifecycle constructor order through the existing transient JsObject serialization boundary. No permanent raw/typed duplicate, new carrier/API, dependency or fallback. New tests compare complete returned JSON strings and stored data bytes, not source structure. Current source/native4-test byte matrix passes12 constructor/return boundaries (`/tmp/storage3-store-wire-green.log`, c4c340b0e); protected strict fixture typecheck passes. This fixes a bounded store wire boundary, not whole-module acceptance.

## Lifecycle and transition listener catch boundary — storage-3 (2026-10-25)

The same frozen catch/log/continue boundary at lines1174–1211 also applies to lifecycle-notification and transition listeners. Both native callbacks now return `Result<(),CallbackError>` and log original throws at their own exact source prefixes before continuing. The expanded existing child-process fixture covers all3 listener kinds × string/null/object throws: actual frozen baseline `/tmp/storage3-store-listeners-source.log`; native API RED `/tmp/storage3-store-listeners-api-red.log` rejected Result callbacks. Waiting, registration, desktop notification and mailbox mirror callers gain only `Ok(())`/early `Ok(())` adapters, preserving their prior local error handling; no outside-lane algorithm changes.

## Update-listener catch boundary (2026-10-25)

`AgentUpdateListener` and `subscribe_agent_updates` now return `Result<(), CallbackError>`. The store catches each rejection, logs `MultiAgentStore agent update listener failed:` with the shared original-throw console formatter, and continues remaining listeners and publication, matching frozen lines1202–1211. `repin_store_listener_failure` compares actual frozen/native state, subsequent-listener revisions and captured stderr for string, null and object throws. API RED `/tmp/storage2-listener-red.log`, frozen baseline `/tmp/storage2-listener-frozen.log`, GREEN `/tmp/storage2-listener-green.log` (1 parent test, 3 child processes). Core deferred-terminal retry propagates its original finalizer error into this catch (the temporary bool drain sink is removed; [proof](terminal-outbox-delivery.md)). Agents-core wait, interactive channel and four test consumers return `Ok(())` mechanically; no callback panic wrapper or unwind catch remains on this boundary. V8 stack frames remain excluded under the standing gap.

## Q18 native AgentNode slice

`lossless_agent_node` records native agentType/permission policy, distinct unknown UTF-16 keys and nested values through DTO serialization and store publication. Committed unexecuted while the coding library is blocked. AgentNode now manually serializes/deserializes through `JsObject`/`JsValue`; unknown fields no longer use serde flatten, and agentType/permission policy/SpawnAgent type are `JsString`. Store JSON has no serde scalar JSON path. Native arbitrary JSON removes the invalid `Eq` promise from AgentNode and containing result types; `PartialEq` remains. Agents-core spawn/attach DTOs, snapshot constructors, inherited policy and lifecycle coordinator callers now pass native type/policy/extra fields directly. Settings profile lookup takes `&JsString`, queries the native configured key, and returns raw profile JSON; [settings traceability](settings-manager.md) tracks the source-backed permissive getter correction and its pending GREEN. Policy-to-enum selection is classification only; persisted policy remains native. Interactive caller updates belong to that active lane. The shared extension integration fixture now constructs native credential/session/action values without changing its existing scalar fixture-input helpers. `agent_current_activity_tagged_fields_round_trip_raw_units` reproduced an in-crate RED (`lossless-agent-node-incrate.log`): serde's internally tagged buffering corrupted raw escapes. `AgentCurrentActivity` now decodes directly from the native object, sharing field extraction with AgentNode/mailbox. Both phases and nested Node/store activity are covered; focused GREEN pending. Runtime proof and remaining fixture propagation are pending.

Separate RED-intent gate: `tracked_read_failure_tests::tracked_agent_read_failure_wakes_with_error_instead_of_disappearing` exercises real DB-open failure from wait notified/terminal-check paths. Production `.ok()?` removal remains queued until executable in-crate RED; this is not acceptance evidence.

Integrated model-lane clippy follow-up: native `MailboxMessageCommandResult::Ok.message` is boxed to avoid a304-byte enum for otherwise ≤32-byte variants. Existing serialization dereferences the box, preserving source-visible result shape. Removed five redundant timestamp borrows in control relocation; no behavior or warning suppression added. Exact integrated clippy recheck belongs to the independent verifier.

## Frozen 4f9e6f8a4 repin — bounded proof

Partial integration only; [integration status and open boundaries](../../specs/control-store-repin.md#integration-status--2026-10-21) are authoritative.

The former [canonical JSON UTF16 counterexample](session-control-db.md#canonical-json-counterexample--bounded-regression-now-green) now passes against the actual frozen producer/reader at `65e23c966`; the old public-body RED is no longer a current blocker. This bounded result does not establish all store restoration/serialization or all payload egress. No whole-module acceptance/coverage advancement is claimed.

Terminal summary/error JS trimming now has actual-source RED→GREEN proof in `repin_control_whitespace`, sharing the JS whitespace helper rather than native Unicode trim.

`repin_store_persistence.rs` compares a real TS store restore/subscription probe against Rust: restore clears transition/lifecycle listener sets but retains update subscribers (RED callback counts1/1/1→GREEN0/0/1). Filesystem-blocked DB allocation/listing propagates errors without local-counter fallback; recovery starts at agent_1 and evicts absent rows. Old terminal snapshots cannot mint fresh notifications, compared with actual TS store. `store-reset-green.log`: differential operation script1 + persistence3 pass after final store changes. No broad acceptance command.

`repin_multi_agent_store.rs` RED5→GREEN5 proves original snapshot returned on consume/removal, controlled relative creation age at 24h−1ms / exactly24h, peer-deleted DB rows evicted without failed-message resurrection, nested tool `detail` preserved and persisted top-level `extra.suppressTerminalNotification` enforced on terminal snapshot publication. `repin_control_agents.rs` additionally proves exact-owner model/activity updates visible to the same TS reader, wrong-owner rejection and typed `detail` roundtrip. The updated actual-source `differential_multi_agent_store` operation script passes including optional effective model publication with current activity.

Source-throwing mailbox listing/consume/projection/publication and counter allocation now return `Result`; persistence errors no longer cause local counter fallback or `.ok()` mutation suppression. Mechanical callers propagate through existing error/logging boundaries. No effective-model producer timing parity is claimed: producer callsites pass absent optional model until their later module-group port.

Remaining: full cross-state reverse-init/mutation and exact schema migration acceptance. Restore/listener/error and reciprocal fixtures above provide bounded evidence, not whole-store acceptance. DB finalization/dead recovery, steering take/consume and retained cleanup have bounded dedicated evidence in [control DB traceability](session-control-db.md). Historical proof below is not renewed whole-module proof. Coverage row unchanged.

Runtime durability remains blocked at the [delivery boundary](../../specs/control-store-repin.md#integration-status--2026-10-21); these fixtures authorize no deployment or live-state use.

Source: `packages/coding-agent/src/core/multi-agent-store.ts` (1467 lines) at the pinned revision.
Rust: `crates/pi-coding-agent-storage/src/core/multi_agent_store.rs`.

## Exported symbols

| TypeScript | Rust | Proof |
|---|---|---|
| `AgentLifecycleState` | `AgentLifecycle` | store tests, differential script |
| `TerminalAgentLifecycleState` | `TerminalAgentLifecycle` | `publish_terminal_outbox_snapshot` branches |
| `SteeringCheckpoint` | `SteeringCheckpoint` | steering tests |
| `MailboxMessageKind` / `Status` | `MailboxMessageKindValue` / `MailboxMessageStatusValue` | mailbox tests |
| `AgentActivity`, `AgentCurrentActivity`, `AgentCurrentActivityOwner` | `AgentActivity`, `AgentCurrentActivity`, `AgentCurrentActivityOwner` | `publish_agent_current_activity` tests |
| `AgentFileReference`, `AgentResult`, `AgentError` | same names | store + control-db tests |
| `AgentWorkerAdapter`, `AgentTranscriptMetadata`, `AgentEventStreamMetadata` | `AgentWorkerAdapter`, `AgentTranscriptMetadata`, `AgentEventStreamMetadata` | transcript tests |
| `AgentNode` / `AgentSnapshot` | `AgentSnapshot` | every store test |
| `SpawnAgentInput` | `SpawnAgent` | `lifecycle_coordinator::prepare_child` |
| `SpawnChildAgentInput`, `AttachSessionAgentInput` | composition via `SpawnAgent` + `AgentOrigin` | `create_attachment` / child tests |
| `AgentMailboxMessage` | `AgentMailboxMessage` | differential store script |
| `SendSteeringInput`, `ContactParentInput`, `SendMailboxMessageInput` | same names | differential store script |
| `AgentCommandResult`, `SpawnChildAgentResult`, `ParentContactResult`, `MailboxMessageCommandResult`, `AgentMetadataCommandResult`, `AgentTranscriptCommandResult`, `AgentViewSelectionResult`, `ActiveAgentTargetSelectionResult` | matching enums with explicit `Serialize` matching TS `{ ok, error, ... }` shapes | differential store script asserts parsed JSON equality per step |
| `MultiAgentStoreOptions` | `MultiAgentStoreOptions` | `now` injection used by differential |
| `AgentLifecycleNotificationListener`, `AgentTransitionListener`, `AgentUpdateListener` | closure types on `subscribe_*` | `multi_agent_store::notifies_subscribers_for_transcript_metadata_updates`, `abort_handlers_fire_once_and_unregister_on_drop` |
| `AgentSlotProjection`, `AgentRowProjection`, `MultiAgentProjectionSnapshot` | same names | projection tests + differential |
| `MultiAgentStore` (class) | `MultiAgentStore` | see method table below |
| `isActiveLifecycle` | `is_active_lifecycle` | `active_counts_follow_core_lifecycle_state` |
| `formatInactiveAgentSelectionMessage` | `format_inactive_agent_selection_message` | `inactive_agents_are_viewable_but_not_active_targets` |

## Method mapping

`new`/`constructor` → `MultiAgentStore::new`; `fromSessionManager` → `from_session_manager`;
`restoreFromSessionManager` → `restore_from_session_manager`; `getPersistenceTarget`,
`setPersistenceSessionManager`, `invalidateInFlightDispatches`, `getRestoreGeneration` map 1:1.

Every other public method maps to the snake_case Rust method of the same name:

`allocateAgentIdForLifecycleCoordinator`, `prepareSteeringMessageForLifecycleCoordinator`,
`publishLifecycleCoordinatorSnapshot`, `publishTerminalOutboxSnapshot`,
`publishLifecycleCoordinatorSteering`, `publishLifecycleCoordinatorSteeringDelivery`,
`publishAgentCurrentActivity`, `selectAgentView(WithStatus)`, `selectActiveAgentTarget(WithStatus)`,
`selectAgentSlot(WithStatus)`, `selectActiveAgentSlotTarget(WithStatus)`, `getSelectedAgentId`,
`clearSelectedAgentView`, `getAgent`, `listAgents`, `listDescendants`, `listMailboxMessages`,
`listPendingMailboxMessagesForAgent`, `markMailboxMessageDelivered`, `markMailboxMessageFailed`,
`consumeCompletionNotificationsForAgent`, `consumeFailureNotificationsForAgent`,
`listPendingLifecycleNotificationsForAgent`, `getProjectionSnapshot`, `listActiveAgents`,
`getActiveAgentCount`, `pinAgentSlot`, `updateAgentTranscript`, `clearAgentSlot`, `contactParent`,
`sendMainThreadMailboxMessage`, `sendMailboxMessage`, `recordOutboundSessionMessage`,
`registerAgentAbortHandler`, `abortAgentHandle`, `subscribeLifecycleNotifications`,
`subscribeAgentTransitions`, `subscribeAgentUpdates`.

## Branches

| Branch | Rust | Proof |
|---|---|---|
| Active lifecycle = `running`, `steering_pending`, `waiting_for_input`, `cancelling` | `is_active_lifecycle` | `active_counts_follow_core_lifecycle_state` |
| Slot selection: pinned slot first, else active agent order | `find_agent_for_slot_selection` | `slot_fallback_uses_active_agent_order`, `pinned_slots_preserve_metadata_and_conflict_with_occupants` |
| Revision fences on metadata mutations | `pin_agent_slot`, `clear_agent_slot`, `update_agent_transcript`, `send_mailbox_message` | `rejects_stale_lifecycle_mutations_and_returns_current_snapshot` |
| Direct messages only between parent/child or main thread children | `can_send_direct_message`, `is_main_thread_child` | `direct_mailbox_messages_require_a_parent_child_relationship` |
| Terminal snapshots record one notification per lifecycle | `retry_or_record_terminal_notification` | `completion_and_failure_notifications_can_be_consumed` |
| Waiting-for-input notifications recorded once | `record_waiting_for_input_notification` | `waiting_for_input_notifications_are_recorded_once` |
| Steering pending → accepted → delivered | `publish_lifecycle_coordinator_steering*` | `steering_messages_move_through_pending_accepted_and_delivered` |
| Restore clears in-flight state and worker handles | `restore_from_session_manager` | `restore_clears_store_when_session_has_no_persisted_state`, `crash_restore_clears_worker_handles_for_active_agents`, `restore_preserves_waiting_notifications_and_mailbox_messages` |
| Persisted counters shared across store instances | `allocate_counter` + `write_multi_agent_counters` | `persisted_counters_are_shared_across_store_instances` |
| Persisted metadata updates survive reopen | `persist_agent_metadata` | `persisted_metadata_updates_survive_reopen` |
| Abort handlers fire once and unregister on drop | `abort_agent_handle` + `AbortSubscription` drop | `abort_handlers_fire_once_and_unregister_on_drop` |
| Listeners on restored sessions are dropped by restore | `invalidate_in_flight_dispatches` / restore | `listeners_registered_on_restored_sessions_are_dropped_by_restore` |

## Differential coverage

`crates/pi-coding-agent/tests/differential_multi_agent_store.rs` drives 28 operations against
`oracle/multi-agent-store-oracle.ts` with a fixed clock: publishing running/completed snapshots,
listing/descendant queries, view and active-target selection, pinned-slot conflict, slot clear,
transcript update, parent contact, mailbox send/pending/deliver, outbound session record,
projection snapshot, agent-id allocation, completion notification consumption, and final state
reads. Outputs are compared as parsed JSON.

## Discrepancies / gaps

- The historical claim that mailbox JSON key order was unobservable was not an approved deviation. Current persistence/wire parity requires the distinct frozen constructor and copyMessage orders; the storage-3 byte-level RED above supersedes that claim. Parsed-value differentials alone do not prove this boundary.
- `private` helpers have no exported symbol; they are exercised indirectly through the public
  methods listed above.
