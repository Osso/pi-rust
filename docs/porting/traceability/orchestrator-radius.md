# Orchestrator Radius
Frozen `packages/orchestrator/src/radius.ts` at [common pin](../source-lock.json).

| Export/branch | Proof |
| --- | --- |
| getRadiusUrl/getRadiusOrchestratorBaseUrl/getRadiusAccessToken/isRadiusEnabled | `credentials_urls_and_bounded_jitter_match_frozen_semantics` |
| RadiusPresence.start/registerPi/disconnectPi/stop | `radius_registration_matches_frozen_fake_fetch`: frozen TS differential with fake fetch and fake timers; registration OS metadata normalized only |
| machine record and registration capabilities | `fake_http_registration_persistence_and_disconnect` |
| three 404 machine heartbeats re-register machine and live Pis | paused Tokio timer behavioral test, RED reproduced self-cancelling callback, corrected pending GREEN |
| machine/Pi backoff, heartbeat success/reset, third 404 recovery, coordinator removal, disconnect 404 | implemented; additional differential failure fixtures pending |

All HTTP uses shared `pi-ai::request::Fetch`; no network in tests. Native credentials reuse coding-agent AuthStorage; injected credential/environment/clock/randomness only provide deterministic boundary values. Timers are single-shot and cleared before callbacks, matching JS clearTimeout on an already-running callback. Unknown/error branch proof remains open; no coverage promotion.

`frozen_radius_machine_and_pi_retry_recovery_matrices`: fake fetch and paused-time native timers versus frozen private heartbeat callbacks; machine transient 500→500→success→404×3→machine+live-Pi recovery, Pi transient500→success→404×3→Pi re-registration. Asserts no request before exact frozen deadline, frozen HTTP body/path sequence, coordinator IDs and queue exhaustion. OS metadata and temp heartbeat paths normalized explicitly, no production data/network.
