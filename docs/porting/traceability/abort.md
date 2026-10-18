# coding-agent abort traceability

Pinned source: `packages/coding-agent/src/utils/abort.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Differential test skipped: behavior depends on promise settlement timing; proven behaviorally in `crates/pi-coding-agent/tests/abort.rs`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `abort.ts:1-7` | Abort reason is `signal.reason`, else an `AbortError`-named `Error("The operation was aborted")` | `utils::abort::AbortError` with the fixed message; focused `rejects_a_pending_operation_when_aborted` |
| `abort.ts:10-12` | `operationSignal` returns the supplied signal or a fresh one | `operation_signal`; focused `operation_signal_returns_the_supplied_token_or_a_fresh_one` |
| `abort.ts:15-18` | No signal returns the operation unchanged | `race_with_abort_signal`; focused `returns_the_operation_value_without_a_signal` |
| `abort.ts:19-22` | Pre-aborted signal rejects without waiting | Focused `rejects_an_already_aborted_operation_without_polling_it` |
| `abort.ts:24-40` | First settlement wins; abort listener is removed; operation errors propagate | `tokio::select!`; focused pending-abort and `propagates_operation_errors` |

## Discrepancies

- `CancellationToken` carries no custom abort reason, so `AbortError` always uses the source default message.
- Rust futures are lazy: a pre-aborted operation is dropped without being polled, whereas a JavaScript promise passed to the source has already started.

## Proof inventory

- Focused cases: 5.
