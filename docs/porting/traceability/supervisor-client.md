# coding-agent supervisor-client traceability

Pinned source: `packages/coding-agent/src/supervisor/client.ts` at `532f7d12`.

Rust: `crates/pi-coding-agent/src/supervisor/client.rs`.

| Source | Rust | Tests |
|---|---|---|
| `RequestSupervisorDecisionInput` | same struct | `tests/supervisor_client.rs` |
| `requestSupervisorDecisionAttempt` (post, wake, poll until deadline) | same | `waits_for_and_returns_the_durable_supervisor_response` |
| `retryDelay` (`base * 2^(attempt-1) * (1 - jitter + random*jitter*2)`) | `retry_delay` (`getrandom` for `Math.random`) | retry test with jitter 0 |
| `requestSupervisorDecision` (maxAttempts default 1, timeout suffix) | same | retry and all-attempts-expire tests |
| `delay` (`setTimeout`) | `tokio::time::sleep` | tests |

## Gaps

- Jitter is random in both implementations; tests pin `retryJitterRatio = 0`, so only the
  deterministic part is compared.
