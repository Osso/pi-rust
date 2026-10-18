# utils/abort-signals traceability

Pinned source: `packages/ai/src/utils/abort-signals.ts` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `combineAbortSignals([])` | No active signal, no-op cleanup. | `utils::abort_signals::combine_abort_signals` returns no token; `combines_no_signals_without_a_token`. |
| `combineAbortSignals([one])` | Returns that signal unchanged, no-op cleanup. | Returns a clone of the token and no bridge tasks; `combines_one_signal_without_a_bridge_task`. |
| `combineAbortSignals(many)` | New controller aborts when any source aborts; an already-aborted source aborts immediately and stops registering later sources; cleanup removes listeners. | Child token plus one bridge task per source; early abort breaks registration. `combines_multiple_signals_into_a_child_token`, `aborts_immediately_when_a_source_signal_is_already_cancelled`, `stops_propagating_after_cleanup`. |
| `cleanup()` | Stops future propagation. | `CombinedAbortSignal::cleanup` aborts bridge tasks; `Drop` also aborts them. |

## Exclusions and deviations

- Combining more than one signal requires a Tokio runtime because each source is observed by a spawned bridge task; TypeScript registers listeners synchronously.
- `controller.abort(signal.reason)` cannot forward a reason through `CancellationToken`.
- No differential oracle: cancellation propagation is scheduling-dependent and TypeScript needs a JS event loop; covered by Tokio tests.
