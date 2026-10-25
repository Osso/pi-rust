# Orchestrator rpc-process

Source: `packages/orchestrator/src/rpc-process.ts` at the [common pin](../source-lock.json). Rust: `crates/pi-orchestrator/src/rpc_process.rs`.
Status: **done** (realistic-input user-observable parity). [Q3 proof and native approvals](../orchestrator-proof.md).

| Export/branch | Behavioral proof |
| --- | --- |
| RpcProcessInstance/createRpcProcessInstance | native spawn method; sibling pi --mode rpc and cwd proved by copied embedding test with sandbox fake sibling |
| send; handleLine; handleUiResponse; pending requests | concurrent IDs, events/UI routing, duplicate ID orphan lifetime, exit/disposal errors, EOF-tail and Node UTF-8 decoding: rpc_process |
| setUiRequestHandler/onEvent/onExit and unsubscribes | frozen Set identity, repeated callback deduplication and unsubscribe counts: rpc_process; raw supervisor stream tests |
| spawn/stdin/exit/dispose | six real spawn failure messages, closed stdin EPIPE, SIGKILL/code errors, disposal pending rejection and fatal→serve cleanup: errors/rpc_process/supervisor |

Duplicate in-flight IDs intentionally keep the overwritten original resolver pending through disposal; orphan senders release only with instance lifetime. Malformed child JSON propagates to serve fatal cleanup without an invented immediate rejection. Both preserve parent-approved frozen behavior.

Proof is bounded to the listed representable runtime/serialization cases; no exhaustive platform or mutation-coverage percentage is claimed.

## Verifier correction (2026-10-21)

`child_exit_is_not_delayed_until_inherited_pipes_close` now notifies in 20ms vs frozen 19ms with a 900ms descendant holding both descriptors. `frame_whitespace.rs` proves 31 child-output padding cases vs Node trim+JSON.parse. Current actual-source fixtures prove inherited-pipe pending rejection/disposal and live event/exit callback mutation; no callback-snapshot gap remains for these APIs.

Prior unconditional done claim withdrawn. Seven regression mutants (exit EOF, queue abort, four trim sites, successful-body read) were killed by behavioral failures, not compiler failures.

Native UTF-16 integration: [lossless orchestrator proof](lossless-orchestrator.md).

Runtime-services whole-module audit: actual frozen listener mutation RED `/tmp/runtime-services-red-rpc-listeners.log` shows JS Set skips a listener removed by the prior callback and visits a listener added by that callback; old Rust snapshots instead called the removed callback and missed the addition. Native event/exit iteration now looks up the next live registration after each callback, outside the state lock. Both event and exit callback traces are compared with the pinned module. GREEN at 7fc5b494b (`/tmp/runtime-services-green-rpc-listeners.log`).

Pending rejection order RED `/tmp/runtime-services-red-rpc-pending-order.log`: eight real child-acknowledged requests are disposed together; frozen Map rejects in registration order while native HashMap wakes waiters in hash order. Pending requests now use insertion-ordered IndexMap, with order-preserving removal and replacement semantics. Test compares observable completion order against frozen; GREEN at f20758e60 (`/tmp/runtime-services-green-rpc-pending-order.log`).

Inherited-pipe proof at 5e0fc66c1 compares pending/not-running error text and prompt rejection/disposal while a descendant holds stdout/stderr for 900 ms (`/tmp/runtime-services-rpc-pipes.log`). Focused duplicate-ID, routing/disposal, EOF/UTF-8 tests also pass at this revision; prior callback and pending-order proofs remain valid. Warning-mode orchestrator clippy exits0 with no orchestrator warnings (`/tmp/runtime-services-clippy-orchestrator.log`). Changed-line manual Rust/JS readability audit passed; metric executable unavailable. Gap: OS-fault shapes, timer/numeric extremes and independent-producer scheduling oddities remain outside scope.
