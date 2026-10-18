# coding-agent management-http traceability

Pinned source: `packages/coding-agent/src/utils/management-http.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.
Rust: `crates/pi-coding-agent/src/utils/management_http.rs` over `pi_ai::request::Fetch` (default client `pi_ai::http::default_fetch`).

Differential test skipped: the source calls the global `fetch`; behavior is proven with a scripted `Fetch` and a real local HTTP server in `crates/pi-coding-agent/tests/management_http.rs`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `RETRYABLE_STATUS_CODES` | 408, 425, 429, 500, 502, 503, 504 | `retries_transient_statuses_until_success`, `does_not_retry_non_transient_statuses` |
| `maxRetries` default 2 | three attempts on transport failure | `retries_a_transient_transport_failure_twice_by_default` |
| `retryOnStatus` | false returns first transient status | `can_disable_status_retries` |
| exhaustion | last status / last error returned | `returns_the_last_transient_status_after_exhausting_retries`, `transport_failure_exhaustion_returns_the_last_error` |
| `signal?.throwIfAborted()` per attempt | caller abort terminal, before and in flight | `caller_cancellation_before_start_never_calls_fetch`, `caller_cancellation_in_flight_is_terminal` |
| `AbortSignal.timeout(timeoutMs)` created once | one budget for all attempts; aborts in-flight attempt; `TimeoutError` message | `timeout_is_one_budget_shared_by_all_attempts_and_terminal`, `zero_timeout_imposes_no_deadline` |
| `response.body?.cancel()` before retry | discarded body dropped | real-server test `default_fetch_retries_a_real_transient_response` |
| `fetch(input, init)` | full request (method/headers/body) forwarded | `HttpRequest` passed unchanged |

## Exclusions / discrepancies
- Source bug: `FetchRetryOptions.timeoutMs` doc comment says "Per-attempt timeout. A new timeout is created for every attempt", but the code creates one `AbortSignal.timeout` before the loop (overall budget, as the function doc says). Rust follows the code.
- Non-finite/fractional `maxRetries` excluded by `Option<usize>` (Aug 12 typed-input decision).
- `error.name === "AbortError" && timeoutSignal === undefined` with no caller abort: unreachable with Rust `Fetch` (aborts only come from the passed signal).
- Custom abort `reason` values are not representable by `CancellationToken`; the default `AbortError`/`TimeoutError` messages are used.
