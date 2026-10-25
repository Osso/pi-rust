# `coding-agent/src/core/lifecycle-coordinator.ts`

Source: `packages/coding-agent/src/core/lifecycle-coordinator.ts` (448 lines) at the pinned
revision. Rust: `crates/pi-coding-agent-storage/src/core/lifecycle_coordinator.rs`.

The coordinator is a thin command layer over `session_control_db::agents`; the DB layer itself is
covered by `session_control_db.md`. All methods take the control DB path from
`LifecycleCoordinatorOptions` and use `now` / `processIdentity` / `sessionPath` from the same
options, mirroring the TS constructor.

## Exported symbols

| TypeScript | Rust | Proof |
|---|---|---|
| `LifecycleCoordinatorOptions` | `LifecycleCoordinatorOptions` | every coordinator test |
| `PrepareChildCommandInput` | `PrepareChildCommandInput` | `prepare_child` tests |
| `CreateChildCommandResult` | `CreateMultiAgentChildWithRuntimeOwnershipResult` (returned by `commit_running_child`) | `atomically_creates_a_running_child_with_runtime_ownership` |
| `CommitFailedChildCommandResult` | `CreateFailedMultiAgentChildResult` (returned by `commit_failed_child`) | `persists_interrupted_construction_as_failed_without_ownership` |
| `CreateAttachmentCommandResult` | `CreateMultiAgentAttachmentResult` | `uses_a_preallocated_artifact_identity_without_consuming_another_id` |
| `AcquireAttachedRuntimeCommandResult` | `AcquireAttachedRuntimeOwnershipResult` | `acquires_attached_runtime_ownership_and_advances_revision` |
| `OwnedLifecycleCommandInput` | `OwnedLifecycleCommandInput` | terminal/cancellation tests |
| `SteeringDeliveryCommandInput` | `SteeringDeliveryCommandInput` | `acknowledge_steering_delivery` |
| `SteeringCommandInput` | `SteeringCommandInput` | `steering_commands_require_registered_sender_and_recipient_listeners` |
| `SteeringCommandResult` | `SteeringCommandResult` | same |
| `DetachedCancellationCommandInput` | `DetachedCancellationCommandInput` | `requests_detached_cancellation_and_enqueues_the_runner_command` |
| `LifecycleCommandResult` | `LifecycleCommandResult` | terminal mutation tests |
| `RecoverDeadChildCommandInput` | `RecoverDeadChildCommandInput` | dead-runtime recovery tests |
| `RecoverDeadChildCommandResult` | `RecoverDeadChildCommandResult` | same |
| `FinalizeChildCommandInput` | `FinalizeChildCommandInput` | finalization tests |
| `LifecycleCoordinator` | `LifecycleCoordinator` | see methods |

## Method mapping

| TypeScript | Rust | Proof |
|---|---|---|
| `constructor` | `LifecycleCoordinator::new` | all tests |
| `createAttachment` | `create_attachment` | `uses_a_preallocated_artifact_identity_without_consuming_another_id` |
| `acquireAttachedRuntime` | `acquire_attached_runtime` | `acquires_attached_runtime_ownership_and_advances_revision`, `rejects_ownership_takeover_while_the_exact_owner_process_is_alive` |
| `confirmChildRuntime` | `confirm_child_runtime` | `atomically_creates_a_running_child_with_runtime_ownership` |
| `acknowledgeSteeringDelivery` | `acknowledge_steering_delivery` | `orders_accepted_cancellation_before_completion_and_deduplicates_acknowledgement` |
| `requestSteering` | `request_steering` | `steering_commands_require_registered_sender_and_recipient_listeners` |
| `markWaitingForInput` | `mark_waiting_for_input` | `blocks_parent_recovery_until_active_descendants_are_terminal` |
| `markDetached` | `mark_detached` | `detach_mark_allows_a_foreign_owner_process`, `rejects_finalization_from_a_different_current_process` |
| `requestCancellation` | `request_cancellation` | `orders_natural_completion_before_a_later_cancellation_request` |
| `requestDetachedCancellation` | `request_detached_cancellation` | `requests_detached_cancellation_and_enqueues_the_runner_command`, `rolls_back_detached_cancellation_when_mailbox_insertion_fails` |
| `acknowledgeCancellation` | `acknowledge_cancellation` | `requires_cancelling_before_abort_acknowledgement` |
| `recoverDeadChild` | `recover_dead_child` | `recovers_a_dead_child_owned_by_a_prior_session_incarnation`, `settles_a_cancelling_child_to_aborted_on_dead_owner_recovery` |
| `finalizeChild` | `finalize_child` | `blocks_parent_terminalization_until_descendants_are_terminal` |
| `failDetachedLaunch` | `fail_detached_launch` | `persists_interrupted_construction_as_failed_without_ownership` |
| `prepareChild` | `prepare_child` | `does_not_persist_a_child_until_construction_succeeds`, `rejects_a_missing_parent_without_committing_child_or_ownership` |
| `commitFailedChild` | `commit_failed_child` | `persists_interrupted_construction_as_failed_without_ownership` |
| `commitRunningChild` | `commit_running_child` | `atomically_creates_a_running_child_with_runtime_ownership` |

## Branches

| Branch | Rust | Proof |
|---|---|---|
| Ownership identity must match the current process and session path | `read_ownership_identity` | `ownership_identity_requires_the_current_process_and_session_path` |
| Terminal parent rejects child/attachment creation | `create_attachment`, `commit_running_child` | `rejects_child_and_attachment_creation_beneath_a_terminal_parent` |
| Dead detached reconciliation retries on SQLite contention after 250 ms | `reconcile_dead_detached_runtimes` + `schedule_dead_detached_reconciliation_retry` | `dead_runtime_reconciliation_is_a_noop_without_candidates` (no-candidate path); retry path is a bounded timer with deduplicated paths |
| Preallocated artifact identity is reused without consuming a new id | `create_attachment` | `uses_a_preallocated_artifact_identity_without_consuming_another_id` |
| Detached cancellation enqueues the runner command atomically | `request_detached_cancellation` | `requests_detached_cancellation_and_enqueues_the_runner_command`, `rolls_back_detached_cancellation_when_mailbox_insertion_fails` |
| Foreign owner process may detach but not finalize | `mark_detached` / `finalize_child` ownership check | `detach_mark_allows_a_foreign_owner_process`, `rejects_finalization_from_a_different_current_process` |
| Supervisor ownership token must belong to the same supervisor | `acquire_attached_runtime` | `rejects_another_agents_ownership_token_under_the_same_supervisor` |

## Differential coverage

The module is deterministic only with a real `control.sqlite` and live process identities, and its
observable effects are the DB mutations covered by `differential_control_db.rs` and
`cross_implementation_control_db.rs`. No separate TS oracle is used; the 23 behavioral tests port
the TS coordinator and lifecycle-authority cases.

## Discrepancies / gaps

- TypeScript command results are structural unions; Rust returns the underlying control-db result
  enums directly where the shapes are identical (`CreateMultiAgentChildWithRuntimeOwnershipResult`,
  `CreateFailedMultiAgentChildResult`, `CreateMultiAgentAttachmentResult`,
  `AcquireAttachedRuntimeOwnershipResult`). Field names and `ok`/`error` tags are unchanged.
- `setTimeout`-based retry is implemented with a detached `std::thread` sleeping 250 ms; there is
  no cancellation token because the TS code also lets the timer run to completion.

## Re-pin audit checkpoint (2026-10-25)

Frozen common pin 4f9e6f8a4 adds DetachedCancellationCommandInput.suppressTerminalNotification. Rust now exposes Option<bool> and forwards it to the existing transactional repository field. Actual coordinator/SQLite behavioral test and frozen coordinator oracle assert the persisted suppression flag; missing-field RED `/tmp/runtime-services-red-lifecycle-suppression.log`, GREEN at 56f10cc47: 24 coordinator tests pass, including actual frozen coordinator suppression (`/tmp/runtime-services-green-lifecycle.log`). Existing callers are mechanically set to None; their suppression-policy integration remains owned by their lanes. Still open: reconciliation artifactRoot/cleanup side effect and retry-root propagation; whole-module row is not done.

Reconciliation RED `/tmp/runtime-services-red-lifecycle-cleanup.log`: repository reconciles one dead detached runtime but expired artifact remains. The coordinator now calls the existing cleanup wrapper when count > 0, forwards the optional artifact root through contention retry, and sends unexpected asynchronous errors to the shared uncaught authority instead of discarding them. Four real DB/filesystem cases compare native/frozen with default and explicit root. GREEN at adde11a9e (`/tmp/runtime-services-green-reconcile.log`). Caller signature propagation in main.rs and agents_core/wait_runtime.rs is mechanical; main forwards its existing agent_dir, matching frozen main.ts:1266.

Timer cancellation RED `/tmp/runtime-services-red-retry-cancel.log`: after a real SQLite writer forces contention, manual successful reconciliation clears the pending timer; old Rust still reopens/recreates a removed DB after 250ms. Retry bookkeeping now stores one identity per pending timer, so clearing cancels it and an old timer cannot remove a newer timer. Frozen/native process-lifetime test checks the observable DB file, not internal callbacks. GREEN pending at commit.
