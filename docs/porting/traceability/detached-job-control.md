# coding-agent detached-job-control traceability

Pinned source: `packages/coding-agent/src/core/detached-job-control.ts` at `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`. Whole-module audit in progress; found incarnation matching divergence and missing explicit createdAt construction.

Rust: `crates/pi-coding-agent-tools/src/core/detached_job_control.rs`. Tests: `crates/pi-coding-agent/tests/detached_job_control.rs` (native real-mailbox behavioral fixtures). Incarnation differential RED: `/tmp/runtime-services-red-control-incarnation.log`; frozen accepts a command with a different incarnation while old native equality drops it. New explicit comparison preserves the candidate identity in the returned command; GREEN pending at commit.

| Source symbol / branch | Rust item | Proof |
| --- | --- | --- |
| `DetachedJobCancelCommand` / `StatusCommand` / `ResponseCommand` / `RuntimeCommand` | Enums/structs with `identity()`, `mailbox_row_id()` | all claim tests |
| `claimDetachedJobRuntimeCommands` (parse, identity match, deliver, fail on mismatch) | `claim_detached_job_runtime_commands` | `accepts_cancellation_from_the_exact_owner_process_and_consumes_the_command`, `rejects_a_bridge_response_without_ownership_identity`, `rejects_commands_for_a_different_runner_identity` |
| `enqueueDetachedJobStatusRequest` (message id, body JSON, sender `main`) | `enqueue_detached_job_status_request` | `accepts_a_status_request_only_from_the_exact_owner_process` |
| `enqueueDetachedJobStatusResponse` | `enqueue_detached_job_status_response` | `enqueues_a_status_response_for_the_requester` |
| `claimDetachedJobControlCommands` (cancel filter) | `claim_detached_job_control_commands` | cancellation test |
| `parseRuntimeCommand` validation (record, identity safe integers, command-specific fields) | `parse_runtime_command`, `parse_identity`, `parse_process_identity`, `parse_mailbox_address` | mismatch tests |
| `commandIdentityMatches` | Explicit job/owner/pid/start/output comparison; incarnation deliberately excluded | different-runner test |
| Failure messages `Detached job control identity mismatch` / `Detached job control delivery failed` | `fail_runtime_mailbox_message` calls | mismatch tests assert the exact message |

## Q18 native control checkpoint

Control/session paths, request IDs and constructed message IDs use JsString. Parsed job/owner/session/output-label/incarnation strings are cloned directly from the lossless JSON authority. Removed scalar_text and every checked UTF-8 drop; command tags compare native literals. Status/result/reason/error payloads preserve raw units and explicit null.

`cancellation_identity_and_reason_keep_raw_units_through_real_mailbox_claim` exercises actual SQLite-backed mailbox insert/claim/deliver with raw identity fields, incarnation and cancellation text. The existing fixtures now use native IDs/paths and JsValue body serialization. All current targets remain **unexecuted** until the real coding library compiles, per main; no mocked-store scaffold and no new runtime PASS.

Gaps / deviations:

- `claimRuntimeMailboxMessages` default limit 20 is inlined as `MAILBOX_CLAIM_LIMIT`.
- `result?: unknown` becomes `Option<JsValue>`; a JSON `null` result is preserved as `Some(Null)`.
- `owner.agentId === null || string` validation rejects a missing `agentId` exactly like the source.
