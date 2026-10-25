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

Successful maybePost disconnect ignores an unreadable 200 body, matching frozen (`radius_body.rs`); reverting status-only success is killed by this assertion. Registration/error bodies remain read. Heartbeat unreadable-body behavior shares the same implementation but has no new direct differential.

Native UTF-16 integration: [lossless orchestrator proof](lossless-orchestrator.md).

Wire-order RED `/tmp/runtime-services-red-radius-wire.log`: parsed-value equality missed native registration JSON placing label/machineId after the fixed fields. Current fixture retains that assertion and additionally compares exact normalized serialization, including machine/Pi labels. Native pure registration-body builders now preserve frozen insertion order; GREEN pending at commit. Only machine-specific hostname/platform/arch/pid values are normalized in the fixture.
