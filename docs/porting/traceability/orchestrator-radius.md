# Orchestrator radius

Source: `packages/orchestrator/src/radius.ts` at the [common pin](../source-lock.json). Rust: `crates/pi-orchestrator/src/radius.rs`.
Status: **partial**. [Q3 proof and native approvals](../orchestrator-proof.md).

| Export/branch | Behavioral proof |
| --- | --- |
| getRadiusUrl/getRadiusOrchestratorBaseUrl/getRadiusAccessToken/isRadiusEnabled | URL/credential precedence native tests; actual fake-fetch frozen registration differential; coding-agent AuthStorage reused |
| RadiusPresence start/stop/registerPi/disconnectPi; registerMachine | frozen HTTP path/body and machine persistence; cached machine short-circuit; timers canceled on stop/disconnect |
| machine/Pi heartbeats; backoff; reRegisterMachineAndPis/reRegisterPi | frozen fake-fetch and paused-time matrices: transient 500 backoff/reset, third 404 recovery, live-Pi coordinator update; no request before exact deadline |
| re-registration failure, coordinator disappearance, ignored disconnect 404 | implemented from source, additional failure-matrix assertions remain unproven |

## Explicit proof gaps
- Re-registration failure, coordinator disappearance and disconnect-404 branches lack complete frozen failure matrices.
