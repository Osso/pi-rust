# utils/abort traceability

Pinned source: `packages/ai/src/utils/abort.ts` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `abortReason(signal)` | Returns `signal.reason` when present, else an `Error` named `AbortError` with message `The operation was aborted`. | `utils::abort::AbortError` with the same message; `races_an_operation_against_cancellation` asserts equality and `Display`. |
| `operationSignal(signal?)` | Returns the provided signal or a fresh never-aborted signal. | `operation_signal(Option<&CancellationToken>)` clones the token or creates a new one; `operation_signal_uses_the_given_token_or_a_fresh_one`. |
| `raceWithAbortSignal(operation, signal)` | Rejects immediately when already aborted without polling the operation; otherwise settles with the first of operation and abort; keeps observing the abandoned promise. | `race_with_abort_signal` uses a biased `tokio::select!` over `signal.cancelled()` and the future; dropping the future replaces promise-rejection observation. `rejects_immediately_without_polling_when_already_cancelled`, `races_an_operation_against_cancellation`, `returns_the_operation_value_when_it_completes_first`. |

## Exclusions and deviations

- `CancellationToken` carries no abort reason, so a custom `signal.reason` value cannot be propagated. The typed error always uses the default `AbortError` message.
- Tie-breaking between a simultaneously ready operation and an aborted signal follows the biased cancellation branch; JavaScript listener/microtask ordering is not reproducible through `CancellationToken`.
- No differential oracle: behavior depends on concurrent scheduling and promise semantics. Covered by deterministic Tokio tests.
