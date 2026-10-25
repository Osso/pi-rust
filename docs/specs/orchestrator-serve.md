# Orchestrator serve

Port of frozen `packages/orchestrator/src/serve.ts` at `4f9e6f8a4`. [Native boundary decisions](../porting/orchestrator-proof.md).

## Requirements

- Start IPC before restart recovery; register Radius only when enabled. Preserve the enabled/disabled, optional machine-id and listening logs.
- On startup failure, close IPC and remove its socket, then propagate the failure.
- Share one shutdown result: close IPC, shut down supervised instances, stop Radius, then remove the socket. Preserve frozen short-circuit failure ordering; do not add retries or extra cleanup.
- Map normal SIGINT/SIGTERM shutdown to exit 0 and fatal errors to exit 1 through the native Result/CLI boundary.

## Proof

[Whole-module audit and proof ledger](../porting/traceability/orchestrator-serve.md), under the 2026-10-25 lighter-proof rule. Native enabled-Radius lifecycle uses isolated Fetch, real IPC/persistence and a fake child; existing native/frozen CLI tests prove both normal signals, logs, failure messages and socket cleanup.

Tests: `tests/runtime_services.rs`, `tests/supervisor.rs`, `tests/cli_serve_process.rs` in `crates/pi-orchestrator`.

## Exclusions

No live Radius/provider calls or user state. Malformed/adversarial inputs and exceptional OS/runtime shapes use the approved trace gap.
