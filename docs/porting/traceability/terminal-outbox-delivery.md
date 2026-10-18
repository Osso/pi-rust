# coding-agent terminal-outbox-delivery traceability

Pinned source: `packages/coding-agent/src/core/terminal-outbox-delivery.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `crates/pi-coding-agent/src/core/terminal_outbox_delivery.rs`. Tests: `crates/pi-coding-agent/tests/terminal_outbox_delivery.rs` (5 behavioral).

| Source symbol / branch | Rust item | Proof |
| --- | --- | --- |
| `TERMINAL_OUTBOX_CLEANUP_INTERVAL_MS` | Same constant | `cleanup_is_due_at_the_hourly_interval` |
| `isTerminalOutboxCleanupDue` | `is_terminal_outbox_cleanup_due` | interval test |
| `terminalOutboxRetentionThreshold` (7 days) | `terminal_outbox_retention_threshold` | retention test |
| `deliverTerminalOutboxProjections` (persistence target gate, claim loop, cleanup after deliveries) | `deliver_terminal_outbox_projections` returning `Result<u32>` | delivery and no-persistence tests |
| `deliverTerminalOutboxProjection` (agent/revision/lifecycle match, publish, acknowledge, fail + rethrow) | `deliver_terminal_outbox_projection`, `project_terminal_outbox` | mismatch test asserts the failure message and pending/attempt state |
| Claim lease 30 s and max attempts 5 | `TERMINAL_OUTBOX_CLAIM_LEASE_MS`, `TERMINAL_OUTBOX_MAX_ATTEMPTS` | claim path in delivery test |
| Cleanup after `delivered > 0` | `run_detached_job_artifact_cleanup` call | over-cap cleanup behavior covered by cleanup tests; delivery test exercises the call path |

Gaps / deviations:

- `deliverTerminalOutboxProjections` returns a `Result`; TS throws after failing the outbox row. The cleanup call still runs whenever at least one projection was delivered.
- `now` is an `Arc<dyn Fn() -> String>`; `Date.parse` uses `pi_js_compat::date::date_parse_millis`.
