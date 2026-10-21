# Orchestrator RPC process
Frozen `packages/orchestrator/src/rpc-process.ts` at [common pin](../source-lock.json).

| Export/branch | Proof |
| --- | --- |
| RpcProcessInstance / createRpcProcessInstance | `tests/rpc_process.rs` real fake child |
| send / correlated response / event / UI dispatch | `routing_matches_frozen_process_and_disposal_rejects_pending` TS differential |
| onEvent, onExit, UI handler, unsubscribes | routing tested; unsubscribe/exit-listener fixtures pending |
| child exit rejects requests; send after exit | `child_response_correlation_events_exit_and_disposal` |
| dispose rejects pending and waits SIGTERM | `routing_matches_frozen_process_and_disposal_rejects_pending` |
| generated IDs, blank lines, partial frames, stderr, malformed JSON | implemented; fixtures pending |

Packaging: parent approved native sibling `pi --mode rpc`, injectable command only for sandbox fakes. Shared coding-agent RPC envelopes re-exported by protocol. No source checkout changes. Frozen process spawn command is replaced on its prototype only inside oracle fixture; routing/lifetime code is unmodified.

Approved frozen discrepancies (parent 2026-10-21): duplicate in-flight ID retains the original pending resolver even after disposal; Rust orphan sender list releases only with instance lifetime. Malformed child JSON propagates through native fatal channel to serve Err/CLI exit 1, with ordinary server/supervisor/Radius/socket cleanup, not a new immediate send rejection. Real-child RED/GREEN and frozen subprocess differential fixtures cover both.

Signal error formatting reuses coding-agent child-process helper. Real child SIGKILL regression fails with numeric 9 before correction; expected frozen signal=SIGKILL.

`tests/errors.rs` differentials reproduce native diagnostic mismatches first: directory read, invalid parent, mkdir-through-file, rm-directory, missing/non-executable/directory executable, absent/file/NUL cwd, and closed child stdin. Shared pi-js-compat Node FS/spawn/pipe-write formatters now retain originating syscall/errno; no JS stacks fabricated. Stdin pipe errors also reach the approved fatal cleanup channel.
