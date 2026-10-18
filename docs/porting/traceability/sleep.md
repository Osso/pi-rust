# coding-agent sleep traceability

Pinned source: `packages/coding-agent/src/utils/sleep.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Differential test skipped: the module depends on real timers, so timing is proven behaviorally in `crates/pi-coding-agent/tests/sleep.rs`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `sleep.ts:4-9` | Already-aborted signal rejects with `Error("Aborted")` | `utils::sleep::sleep` returns `SleepError::Aborted`; focused `rejects_immediately_when_already_aborted` |
| `sleep.ts:11` | Resolve after `ms` milliseconds | Focused `sleeps_for_the_requested_duration_without_a_signal` |
| `sleep.ts:13-16` | Abort clears the timer and rejects | `tokio::select!` over `sleep` and `CancellationToken::cancelled`; focused `aborts_a_pending_sleep` |

## Discrepancies

- `AbortSignal` maps to `tokio_util::sync::CancellationToken`; the error is a typed enum instead of `Error("Aborted")`.

## Proof inventory

- Focused cases: 3 (duration, pre-aborted, pending abort).
