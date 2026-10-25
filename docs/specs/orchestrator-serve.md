# Orchestrator serve

Port of frozen `packages/orchestrator/src/serve.ts`. Native boundary decisions and proof scope: [Q3 proof](../porting/orchestrator-proof.md).

## What it must do
- [x] serve startup; server/recovery/Radius setup: native start startup-failure cleanup, frozen startup logs and source SIGTERM exit-0 socket cleanup: supervisor.
- [x] shutdown promise/idempotence; uncaughtException/unhandledRejection mapping: native idempotent cleanup; frozen malformed-child exit 1 and native fatal-channel Err, disposed pending send, removed instances/socket: supervisor.
- [ ] Direct native embedding-process SIGINT/SIGTERM and Radius-enabled full serve startup/shutdown matrices remain unproven.

## How it works
- [Module traceability](../porting/traceability/orchestrator-serve.md)

## Implementation inventory
- `crates/pi-orchestrator/src/serve.rs`: native module.

## Tests asserting this spec
- `crates/pi-orchestrator/tests/supervisor.rs`
- `crates/pi-orchestrator/tests/errors.rs`

## Known gaps (current cycle)
- [ ] Direct native embedding-process SIGINT/SIGTERM and Radius-enabled full serve startup/shutdown matrices remain unproven.

## Out of scope
Live services, user state and real Radius network calls; native packaging follows the approved library boundary.

Startup directory failures preserve the frozen Node filesystem error message, including operation and path; the native CLI embedding returns exit1.
