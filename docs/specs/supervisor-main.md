# Supervisor main

Port `packages/coding-agent/src/supervisor/main.ts` at frozen `4f9e6f8a4`. [Traceability](../porting/traceability/supervisor-main.md) records current proof and implementation details.

## What it must do

- [x] Select authenticated `gpt-6.1-sol` models in frozen primary/fallback order and preserve the no-auth error.
- [x] Prepare the resident session, retaining the frozen selected file, session ID, cwd and archive behavior.
- [ ] Preserve request/console priority, durable completion and cancellation, prompt lifetime and exact current-response errors.
- [x] Review non-generic goal continuation instructions; preserve accepted instructions, suppress rejected/invalid/failed reviews to the frozen generic continuation, and append exact hidden policy feedback before completion.
- [ ] Preserve stateless review authentication/fallback, console identity, resource loading and service cleanup.

## How it works

- [Module branch map](../porting/traceability/supervisor-main.md)

## Implementation inventory

- `crates/pi-coding-agent/src/supervisor/main.rs`: Supervisor configuration, sessions, policy review and runtime loop.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/differential_supervisor_main.rs`
- `crates/pi-coding-agent/tests/differential_supervisor_policy.rs`
- `crates/pi-coding-agent/tests/differential_supervisor_request_loop.rs`
- `crates/pi-coding-agent/tests/supervisor.rs`

## Known gaps (current cycle)

- [ ] Finish the request-policy branch matrix and stateless evaluator/service wiring.
- [ ] Finish snapshot identity and lifecycle proofs.

## Out of scope

Adversarial inputs, OS/runtime fault shapes and scheduling oddities follow the approved project parity exclusions.
