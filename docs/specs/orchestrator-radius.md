# Orchestrator radius

Port of frozen `packages/orchestrator/src/radius.ts`. Native boundary decisions and proof scope: [Q3 proof](../porting/orchestrator-proof.md).

## What it must do
- [x] Successful heartbeat/disconnect (maybePost) validates status without consuming the response body; errors still read text and registrations still read JSON.
- [x] getRadiusUrl/getRadiusOrchestratorBaseUrl/getRadiusAccessToken/isRadiusEnabled: URL/credential precedence native tests; actual fake-fetch frozen registration differential; coding-agent AuthStorage reused.
- [x] RadiusPresence start/stop/registerPi/disconnectPi; registerMachine: frozen HTTP path/body and machine persistence; cached machine short-circuit; timers canceled on stop/disconnect.
- [x] machine/Pi heartbeats; backoff; reRegisterMachineAndPis/reRegisterPi: frozen fake-fetch and paused-time matrices: transient 500 backoff/reset, third 404 recovery, live-Pi coordinator update; no request before exact deadline.
- [ ] Re-registration failure, coordinator disappearance and disconnect-404 branches lack complete frozen failure matrices.

## How it works
- [Module traceability](../porting/traceability/orchestrator-radius.md)

## Implementation inventory
- `crates/pi-orchestrator/src/radius.rs`: native module.

## Tests asserting this spec
- `crates/pi-orchestrator/tests/radius.rs`

## Known gaps (current cycle)
- [ ] Re-registration failure, coordinator disappearance and disconnect-404 branches lack complete frozen failure matrices.

## Out of scope
Live services, user state and real Radius network calls; native packaging follows the approved library boundary.

Machine and Pi registration request JSON retains frozen field insertion order, including optional machineId/label/sessionId positions, not only equal parsed values.
