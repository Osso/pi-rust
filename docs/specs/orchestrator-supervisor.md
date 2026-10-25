# Orchestrator supervisor

Port of frozen `packages/orchestrator/src/supervisor.ts` at `4f9e6f8a4`. [Native boundary decisions](../porting/orchestrator-proof.md).

## Requirements

- Preserve live/stored instance updates, owned record returns, status timestamps and restart recovery ordering.
- Preserve spawn/session-sync/Radius registration, failed-spawn finalization, unexpected-exit diagnostics and ordered shutdown.
- Preserve event/UI subscriptions and shared callback identity. Refresh session metadata after exactly new_session, switch_session, fork, clone, set_session_name and prompt.
- Preserve frozen cleanup failure ordering: a failed Radius disconnect skips RPC disposal, while stopInstance still removes its live/stored record. Do not repair this source bug silently.

## Proof

[Whole-module audit and proof ledger](../porting/traceability/orchestrator-supervisor.md), under the 2026-10-25 lighter-proof rule. Existing frozen lifecycle/stream and reciprocal IPC tests remain; a native enabled-Radius lifecycle and failed-disconnect test exercise the resource boundary. No exhaustive metadata-command matrix is required.

Tests: `tests/runtime_services.rs`, `tests/supervisor.rs`, `tests/rpc_process.rs`, `tests/ipc.rs` in `crates/pi-orchestrator`.

## Exclusions

No live Radius/provider calls or user state. Malformed/adversarial inputs and exceptional OS/runtime shapes use the approved trace gap.
