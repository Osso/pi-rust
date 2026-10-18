# `coding-agent/src/core/multi-agent-store.ts`

Source: `packages/coding-agent/src/core/multi-agent-store.ts` (1387 lines) at the pinned revision.
Rust: `crates/pi-coding-agent/src/core/multi_agent_store.rs`.

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
| Active lifecycle = `spawning`, `running`, `steering_pending`, `waiting_for_input`, `cancelling` | `is_active_lifecycle` | `active_counts_follow_core_lifecycle_state` |
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

- TypeScript builds mailbox objects with different key orders at different creation sites
  (terminal notification, parent contact, outbound record). Rust uses one struct field order.
  JSON object key order is not part of the observable contract: every consumer parses the payload
  and TS itself is internally inconsistent. The differential therefore compares parsed values.
- `private` helpers have no exported symbol; they are exercised indirectly through the public
  methods listed above.
