# Orchestrator rpc-process

Port of frozen `packages/orchestrator/src/rpc-process.ts`. Native boundary decisions and proof scope: [Q3 proof](../porting/orchestrator-proof.md).

## What it must do
- [x] Notify exit and complete disposal on child exit without waiting for inherited stdout/stderr EOF. Child frame padding uses ECMAScript String.trim.
- [x] RpcProcessInstance/createRpcProcessInstance: native spawn method; sibling pi --mode rpc and cwd proved by copied embedding test with sandbox fake sibling.
- [x] send; handleLine; handleUiResponse; pending requests: concurrent IDs, events/UI routing, duplicate ID orphan lifetime, exit/disposal errors, EOF-tail and Node UTF-8 decoding: rpc_process.
- [x] setUiRequestHandler/onEvent/onExit and unsubscribes: frozen Set identity, repeated callback deduplication and unsubscribe counts: rpc_process; raw supervisor stream tests.
- [x] spawn/stdin/exit/dispose: six real spawn failure messages, closed stdin EPIPE, SIGKILL/code errors, disposal pending rejection and fatal→serve cleanup: errors/rpc_process/supervisor.

## How it works
- [Module traceability](../porting/traceability/orchestrator-rpc-process.md)

## Implementation inventory
- `crates/pi-orchestrator/src/rpc_process.rs`: native module.

## Tests asserting this spec
- `crates/pi-orchestrator/tests/rpc_process.rs`
- `crates/pi-orchestrator/tests/errors.rs`
- `crates/pi-orchestrator/tests/supervisor.rs`

## Known gaps (current cycle)
No additional in-slice gap beyond the proof boundary above.

## Out of scope
Live services, user state and real Radius network calls; native packaging follows the approved library boundary.

Event and exit subscriptions follow live Set iteration: callbacks removed before their turn are skipped, and newly appended callbacks are visited during the current emission. Callback code runs without the transport state lock.
