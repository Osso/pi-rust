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

Wire-order RED `/tmp/runtime-services-red-radius-wire.log`: parsed-value equality missed native registration JSON placing label/machineId after the fixed fields. Current fixture retains that assertion and additionally compares exact normalized serialization, including machine/Pi labels. Native pure registration-body builders now preserve frozen insertion order; GREEN at 3419a6f47 (`/tmp/runtime-services-green-radius-wire.log`). Only machine-specific hostname/platform/arch/pid values are normalized in the fixture.

Recovery error RED `/tmp/runtime-services-red-radius-failures.log`: a Pi registration HTTP503 during machine recovery loses the frozen `HTTP 503:` diagnostic prefix. The internal Pi registration path now retains RadiusError/status through re-registration; public register_pi still returns its original message-only Result. One formatter serves all retry/nested-recovery logs. Owned-process fake-Fetch matrix checks exact console bytes, normalized serialized requests, absent-Pi timer cancellation, machine/Pi recovery failure and ignored disconnect404. No live provider or global host state is used. GREEN at a9534b8bd (`/tmp/runtime-services-green-radius-failures.log`).

Invalid-URL RED `/tmp/runtime-services-red-radius-url.log`: an ordinary malformed base URL leaked url-crate reason text instead of Node `Invalid URL`. Both base derivation and explicit-base POST URL validation now preserve the frozen message. Native/frozen public start tests cover both settings; GREEN at 1a1adf3e4 (`/tmp/runtime-services-green-radius-url.log`).

Jitter-consumption RED `/tmp/runtime-services-red-radius-random.log`: a 404 incorrectly consumes randomness, changing the following 500 retry from frozen 1025ms to native 1225ms under a concrete two-draw fixture. The decision helper now requests a random draw only in the transient branch, preserving frozen control flow. Fifth owned-process case compares exact logged delay; GREEN pending at commit.
