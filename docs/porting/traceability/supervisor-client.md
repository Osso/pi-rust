# supervisor/client.ts

Frozen `4f9e6f8a4`; Rust `crates/pi-coding-agent/src/supervisor/client.rs`.

| Export / branch | Proof |
|---|---|
| cancellation reason, default poll interval | Exact frozen literals; cancellation response differential |
| `RequestSupervisorDecisionInput`, `SupervisorClientDependencies` | Native optional CancellationToken and injected startup closure; compiled callers |
| `requestSupervisorDecision`: pre-abort, startup failure, abort during startup, poll abort, completed response, retry-delay abort, timeout suffix, zero attempts | `q16_client::startup_cancellation_retry_and_completion_match_frozen_rows`: eight cases comparing actual frozen responses, startup effects and durable DB rows |
| default startup dependency | `ensure_running` module; no live service started by tests |
| retry delay | Existing exponential/jitter formula retained; deterministic tests pin jitter ratio to zero |
| delay cancellation / stored cancellation | `supervisor_client` five fixtures, plus frozen polling/retry cancellation matrix |

Random jitter/non-finite timer inputs are not exhaustively differential. Mechanical `signal: None` integration in callers does not prove their cancellation threading. [Slice ledger](../q16-supervisor-runtime-proof.md).
