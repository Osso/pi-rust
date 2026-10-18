# coding-agent detached-job-control traceability

Pinned source: `packages/coding-agent/src/core/detached-job-control.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `crates/pi-coding-agent/src/core/detached_job_control.rs`. Tests: `crates/pi-coding-agent/tests/detached_job_control.rs` (6 behavioral).

| Source symbol / branch | Rust item | Proof |
| --- | --- | --- |
| `DetachedJobCancelCommand` / `StatusCommand` / `ResponseCommand` / `RuntimeCommand` | Enums/structs with `identity()`, `mailbox_row_id()` | all claim tests |
| `claimDetachedJobRuntimeCommands` (parse, identity match, deliver, fail on mismatch) | `claim_detached_job_runtime_commands` | `accepts_cancellation_from_the_exact_owner_process_and_consumes_the_command`, `rejects_a_bridge_response_without_ownership_identity`, `rejects_commands_for_a_different_runner_identity` |
| `enqueueDetachedJobStatusRequest` (message id, body JSON, sender `main`) | `enqueue_detached_job_status_request` | `accepts_a_status_request_only_from_the_exact_owner_process` |
| `enqueueDetachedJobStatusResponse` | `enqueue_detached_job_status_response` | `enqueues_a_status_response_for_the_requester` |
| `claimDetachedJobControlCommands` (cancel filter) | `claim_detached_job_control_commands` | cancellation test |
| `parseRuntimeCommand` validation (record, identity safe integers, command-specific fields) | `parse_runtime_command`, `parse_identity`, `parse_process_identity`, `parse_mailbox_address` | mismatch tests |
| `commandIdentityMatches` | `StoredCommand::identity` equality with `DetachedJobOwnershipIdentity` | different-runner test |
| Failure messages `Detached job control identity mismatch` / `Detached job control delivery failed` | `fail_runtime_mailbox_message` calls | mismatch tests assert the exact message |

Gaps / deviations:

- `claimRuntimeMailboxMessages` default limit 20 is inlined as `MAILBOX_CLAIM_LIMIT`.
- `result?: unknown` becomes `Option<Value>`; a JSON `null` result is preserved as `Some(Null)`.
- `owner.agentId === null || string` validation rejects a missing `agentId` exactly like the source.
