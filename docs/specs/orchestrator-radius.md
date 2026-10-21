# Orchestrator Radius
Port frozen Radius HTTP registration, credential lookup and timed presence recovery.

## What it must do
- [x] Register machine/Pi with frozen request bodies and persistent machine record.
- [x] Resolve URLs and credential precedence and bound jitter/backoff.
- [x] Disconnect Pi and machine, cancel pending timers.
- [ ] Reset retry counters on success; re-register after three 404s; retry transient failures with frozen delays.

## How it works
- [Traceability](../porting/traceability/orchestrator-radius.md)

## Implementation inventory
- `crates/pi-orchestrator/src/radius.rs`: injectable Fetch/timer state.

## Tests asserting this spec
- `crates/pi-orchestrator/tests/radius.rs`

## Known gaps (current cycle)
- [ ] Complete failure and heartbeat branch differential matrix.

## Out of scope
Live Radius requests and user credentials.
