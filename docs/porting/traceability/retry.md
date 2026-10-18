# retry traceability

Source: `packages/ai/src/utils/retry.ts`.
Rust: `crates/pi-ai/src/utils/retry.rs`.
Tests: `crates/pi-ai/tests/retry.rs` (10 behavioral), `crates/pi-ai/tests/differential_retry.rs` (45 classifications + 10 scripted loops vs pinned TS).

## Representation
- `RetryPolicy` fields keep TS names (`enabled`, `max_retries: u32`, `base_delay_ms: f64`).
- `AbortSignal` → `Option<&CancellationToken>`; `sleep` is a `tokio::select!` between `tokio::time::sleep` and `token.cancelled()`.
- Callbacks are a `RetryCallbacks` trait with RPITIT futures and default no-op bodies; callers pass `Option<&mut impl RetryCallbacks>`.
- `produce` is `FnMut() -> Future<Output = AssistantMessage>`.

## Symbols → proof
| TS | Rust | Test |
|---|---|---|
| `isRetryableAssistantError` | `is_retryable_assistant_error` | `classifies_provider_retry_errors` + 45 differential classifications |
| `NON_RETRYABLE_PROVIDER_ERROR_PATTERN` (10 strings) | `NON_RETRYABLE_PROVIDER_ERROR_PATTERNS` | same |
| `RETRYABLE_PROVIDER_ERROR_PATTERN` (43 strings) | `RETRYABLE_PROVIDER_ERROR_PATTERNS` | same |
| `retryAssistantCall` success path | `retry_assistant_call` | `successful_response_returns_immediately_without_retrying` |
| aborted response is terminal | `should_retry` | `aborted_message_is_not_retried` |
| non-retryable error fails fast | `should_retry` | `non_retryable_error_returns_immediately` |
| bounded retries + final error | `retry_assistant_call` | `retries_transient_errors_up_to_max_retries_then_returns_the_final_error` |
| success after retries | `finish_assistant_call` | `stops_retrying_once_a_call_succeeds` |
| aborted retried call reports failure | `finish_assistant_call` | `reports_an_aborted_retried_call_as_unsuccessful` |
| disabled policy | `retry_assistant_call` | `disabled_policy_does_not_retry` |
| `onRetryScheduled` / `onRetryAttemptStart` ordering | `retry_assistant_call` | `attempt_start_is_emitted_after_backoff_before_each_retried_call`, differential loops |
| `onRetryFinished` arguments | `finish_assistant_call`, `wait_for_retry` | all loop tests + differential |
| abort during backoff → aborted message, `errorMessage: undefined` | `wait_for_retry` | `aborting_backoff_returns_an_aborted_message_and_reports_failure` |
| `errorMessage \|\| "Unknown error"` | `retry_assistant_call` (empty filter) | differential id 54 (`errorMessage: ""`) |

## Branch notes
- `baseDelayMs * 2^(attempt-1)`; delays are converted to `Duration` from seconds, clamping non-positive values to zero.
- The first `produce()` is never guarded by the abort signal (matches TS).
- Classification uses `(?i)` alternation of the exact TS strings; non-retryable patterns win before retryable patterns.
- A missing `errorMessage` or `stopReason != "error"` is never retryable.

## Gaps
- TypeScript's `produce` can reject; Rust futures cannot carry a typed throw, so rejected produces surface as panics. Callers must return an error `AssistantMessage` as TS callers do.
- `RetryCallbacks` is generic rather than a dynamic object; no behavioral difference.

## Discrepancies
- None.
