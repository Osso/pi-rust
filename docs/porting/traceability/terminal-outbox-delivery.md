# coding-agent terminal-outbox-delivery traceability

Current source: `packages/coding-agent/src/core/terminal-outbox-delivery.ts` at frozen `4f9e6f8a4`. Status: **PARTIAL**. The historical matrix below is not renewed whole-module proof.

## Stage2 numeric/clock and caller slice2026-10-25

`repin_storage_terminal_outbox` compile-RED→GREEN covers40 actual-source cadence/threshold combinations (fractional, clip-range, NaN/±Infinity clocks and absent/NaN last-cleanup) plus6 real persisted-session clock/gate scenarios. Both native helpers retain f64 domains; threshold and claim lease use shared `date::to_iso_string_value` with RangeError branding. `now` returns `Result<JsString,CallbackError>`, and original Error/non-Error callback values survive. An invalid Date.parse throws before claiming rather than omitting the lease/defaulting cleanup to0. Delivered count is a JS f64 counter. Cleanup's existing `now` option/run signature widens to f64, preserving NaN without changing retention algorithms or quarantine behavior.

Claim/fail clock strings cross to existing scalar SQL parameters only where frozen binds them as TEXT (`session-control-db.ts:3527-3540,3567-3568,3596-3609`); these fields do not enter arbitrary JSON. No human-text scalar projection occurs before that source conversion.

Caller map:

- `agent-session.ts:3503-3519`: native drain returns Result and propagates cleanup/threshold/delivery errors.
- `agent-session.ts:3492-3495`: coordination catches/logs exactly `Failed to drain terminal outbox projections: <errorMessage>` and continues.
- Frozen has only two direct drain callers (`1892`, `3493`); obsolete native prompt-entry draining is removed. `tests/agent_session.rs::registered_commands_precede_terminal_outbox_drain_and_normal_prompt_coordinates_afterward` compares real source/native sessions, registered extension commands, faux turns and SQLite outbox rows. RED native attempts after command/start/end1/2/4 vs frozen0/0/2; GREEN0/0/2. No mocked session method or live provider.
- `extensions/agents-core/src/runtime.ts:1837-1852` propagates publish/delivery; `2244-2253` reuses one captured nowIso during detached reconciliation. Mechanical native callers preserve the carrier; existing HandlerError conversion keeps the original callback.

**Must close at stage4:** parent-designated temporary native bool wrapper at reserved-finalize retains its pre-existing drain-error sink. This is not an approved parity deviation and blocks affected whole-module closure. Migrate AgentUpdateListener to Result and apply frozen store-local catch/log at `multi-agent-store.ts:1202-1211`, then replace the bool wrapper and prove `agent-session.ts:1892` propagation. No new uncaught carrier or fallback was added.

Logs: `/tmp/pi-rust-repin-storage/terminal-outbox-{compile-red,green}.log`, `prompt-outbox-order-{red-attempt,green}.log`. Remaining: reciprocal real terminal-row claim/publication/ack/failure/cleanup and full source branch proof; the earlier5 native tests alone do not establish it.

Rust: `crates/pi-coding-agent-storage/src/core/terminal_outbox_delivery.rs`. Tests: `crates/pi-coding-agent/tests/terminal_outbox_delivery.rs` (5 behavioral).

| Source symbol / branch | Rust item | Proof |
| --- | --- | --- |
| `TERMINAL_OUTBOX_CLEANUP_INTERVAL_MS` | Same constant | `cleanup_is_due_at_the_hourly_interval` |
| `isTerminalOutboxCleanupDue` | `is_terminal_outbox_cleanup_due` | interval test |
| `terminalOutboxRetentionThreshold` (7 days) | `terminal_outbox_retention_threshold` | retention test |
| `deliverTerminalOutboxProjections` (persistence target gate, claim loop, cleanup after deliveries) | `deliver_terminal_outbox_projections` returning `Result<f64,CallbackError>` | delivery and no-persistence tests |
| `deliverTerminalOutboxProjection` (agent/revision/lifecycle match, publish, acknowledge, fail + rethrow) | `deliver_terminal_outbox_projection`, `project_terminal_outbox` | mismatch test asserts the failure message and pending/attempt state |
| Claim lease 30 s and max attempts 5 | `TERMINAL_OUTBOX_CLAIM_LEASE_MS`, `TERMINAL_OUTBOX_MAX_ATTEMPTS` | claim path in delivery test |
| Cleanup after `delivered > 0` | `run_detached_job_artifact_cleanup` call | over-cap cleanup behavior covered by cleanup tests; delivery test exercises the call path |

Historical shape notes are superseded by the current Result/f64/native-callback contract above. Whole-module acceptance remains blocked by the explicit stage4 dependency and pending terminal-row differential matrix, not waived as a Result-vs-throw deviation.
