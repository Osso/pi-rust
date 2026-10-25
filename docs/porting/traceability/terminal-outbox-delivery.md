# coding-agent terminal-outbox-delivery traceability

Current source: `packages/coding-agent/src/core/terminal-outbox-delivery.ts` at frozen `4f9e6f8a4`. Status: **DONE for current user-observable parity** after the storage-3 whole84-line audit below. Whole control/store integration acceptance remains separate.

## Stage2 numeric/clock and caller slice2026-10-25

`repin_storage_terminal_outbox` compile-RED→GREEN covers40 actual-source cadence/threshold combinations (fractional, clip-range, NaN/±Infinity clocks and absent/NaN last-cleanup) plus6 real persisted-session clock/gate scenarios. Both native helpers retain f64 domains; threshold and claim lease use shared `date::to_iso_string_value` with RangeError branding. `now` returns `Result<JsString,CallbackError>`, and original Error/non-Error callback values survive. An invalid Date.parse throws before claiming rather than omitting the lease/defaulting cleanup to0. Delivered count is a JS f64 counter. Cleanup's existing `now` option/run signature widens to f64, preserving NaN without changing retention algorithms or quarantine behavior.

Claim/fail clock strings cross to existing scalar SQL parameters only where frozen binds them as TEXT (`session-control-db.ts:3527-3540,3567-3568,3596-3609`); these fields do not enter arbitrary JSON. No human-text scalar projection occurs before that source conversion.

Caller map:

- `agent-session.ts:3503-3519`: native drain returns Result and propagates cleanup/threshold/delivery errors.
- `agent-session.ts:3492-3495`: coordination catches/logs exactly `Failed to drain terminal outbox projections: <errorMessage>` and continues.
- Frozen has only two direct drain callers (`1892`, `3493`); obsolete native prompt-entry draining is removed. `tests/agent_session.rs::registered_commands_precede_terminal_outbox_drain_and_normal_prompt_coordinates_afterward` compares real source/native sessions, registered extension commands, faux turns and SQLite outbox rows. RED native attempts after command/start/end1/2/4 vs frozen0/0/2; GREEN0/0/2. No mocked session method or live provider.
- `extensions/agents-core/src/runtime.ts:1837-1852` propagates publish/delivery; `2244-2253` reuses one captured nowIso during detached reconciliation. Mechanical native callers preserve the carrier; existing HandlerError conversion keeps the original callback.

**Closed2026-10-25:** Result update subscribers now catch/log in the store at the frozen boundary. Reserved-finalize returns `Result<bool,CallbackError>`; ownership, coordinator and terminal-outbox drain failures propagate unchanged through direct steering completion, prompt/continue, or the deferred update subscriber. The temporary bool error sink is removed. `agent_session_runtime_mailbox::finalize::reserved_finalize_propagates_outbox_claim_failure_after_durable_finalization` compares the actual frozen private finalizer using a real manager/store/coordinator/SQLite trigger against a native full faux prompt→steering→durable finalize→claim failure. RED native prompt incorrectly returned Ok; GREEN returns the exact frozen `fixture claim failure`, with terminal persistence completed and outbox still pending (`/tmp/storage2-finalize-{red,green}.log`). Separate source/native child stderr/state proof covers store-local catch and continued listeners, [store trace](multi-agent-store.md#update-listener-catch-boundary-2026-10-25). No new uncaught carrier or fallback.

Logs: `/tmp/pi-rust-repin-storage/terminal-outbox-{compile-red,green}.log`, `prompt-outbox-order-{red-attempt,green}.log`. The storage-3 matrix below now supplies the previously missing reciprocal terminal-row proof; earlier5 native tests alone were not acceptance evidence.

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

## Whole-module reciprocal audit — storage-3 (2026-10-25)

All84 frozen lines, three public functions, options and private projection/lifecycle branches were compared with native. New `repin_storage_terminal_matrix` invokes actual frozen public store/control/delivery APIs against private persistent DBs and TS-created JSONL:

| Capability | Current source/native proof |
|---|---|
| claim, terminal lifecycle guard, publication, exact snapshot, acknowledge | TS-produced rows→native delivery→same TS read, then native-produced rows→TS delivery→native read for completed/failed/aborted;6 producer/delivery combinations |
| missing agent, wrong revision, nonterminal state | same real TS terminal row, actual peer-state mutation and both consumers; exact error and pending/poisoned row at attempts1 and5 |
| acknowledgement CAS failure | real SQLite before-delete peer claim replacement/ignored deletion; exact original error, claimed status and peer ownership preserved through guarded fail |
| cleanup finally after positive delivery | actual expired private detached output directory is removed after every successful reciprocal projection |
| later loop-clock failure and cleanup-clock failure | original TypeError/code survives after earlier acknowledgement; later-loop failure still deletes artifact, cleanup-clock rejection leaves artifact; exact callback count3 and projected snapshot |

`cargo test -j4 -p pi-coding-agent --test suite_session_1 repin_storage_terminal -- --nocapture` passes5 tests (3 new matrix groups plus2 earlier helper/gate groups), `/tmp/storage3-terminal-matrix-green.log`, revision8d8d08a98. No terminal production change was needed. Initial compile/carrier fixture failures were not production REDs: Rust operation inputs intentionally lack serde; full JsValue comparison models JS binary64 equality for1/1.0 while retaining every result field. Existing helper40 vectors, clock/gate6 cases and reserved-finalize source/native failure proof remain unchanged.

Protected strict no-emit fixture typecheck and Biome pass; warning-mode scoped test Clippy passes with zero diagnostics in the new terminal/archive test files. Manual changed-line Rust/TS readability audit found no new structural issue; metrics executable unavailable. Source rows remain locked; exact revision/command ledger `/tmp/storage3-proof-ledger.md`. Historical shape notes are superseded by the current Result/f64/native-callback contract above.

Gap: exceptional OS/runtime faults, adversarial persisted shapes and numeric extremes follow current parity exclusions; no new fault carriers or scheduling emulation.
