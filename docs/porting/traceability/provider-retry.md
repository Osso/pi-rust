# utils/provider-retry traceability

Q18 human-error representation and bounded proof: [lossless provider errors](lossless-provider-errors.md). Earlier counts are historical, not rerun evidence.

Pinned source: `packages/ai/src/utils/provider-retry.ts` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `DEFAULT_MAX_RETRY_DELAY_MS` | `60_000`. | `provider_retry::DEFAULT_MAX_RETRY_DELAY_MS`. |
| `isProviderError` / `ProviderError` | Error with optional numeric `status` and `Headers`. | `ProviderError` + case-insensitive `ProviderErrorHeaders`; non-provider throws use `ProviderRequestError::Other`. `headers_are_case_insensitive`. |
| `isRetryableProviderError` | `x-should-retry: true/false` wins; absent status retries; 408/409/429/>=500 retry. | `is_retryable_provider_error`; `retries_errors_marked_retryable_even_for_client_statuses`, `does_not_retry_errors_marked_non_retryable`, `retries_statuses_without_an_explicit_retry_header`. |
| `validateServerRetryDelayMs` | Cap defaults to 60s; `maxRetryDelayMs = 0` disables; over-cap throws `Server requested Ns retry delay (max: Ms). <message>` with ceiling seconds. | `validate_server_retry_delay_ms` + `RetryDelayLimitError`; `rejects_a_provider_requested_delay_above_the_limit`, `allows_disabling_the_delay_cap`, `retry_delay_limit_display_uses_ceiling_seconds`. |
| `getRetryDelayMs` | `retry-after-ms` parseFloat; `retry-after` parseFloat seconds or `Date.parse`; otherwise `min(0.5 * 2^index, 8) * 1000 * (1 - random * 0.25)`. | `get_retry_delay_ms`, `pi_js_compat::number::parse_float`, `pi_js_compat::date::date_parse`, `random_fraction`; `non_finite_retry_after_values_exceed_the_cap`, `iso_8601_prefixes_parse_as_seconds_like_parse_float`, `exponential_delay_is_capped_at_eight_seconds`, `server_delays_are_validated_against_the_cap`, `honors_http_date_retry_after_headers`; helper values in `crates/pi-js-compat/tests/number.rs` and `date.rs`. |
| `abortableSleep` | Sleep is interruptible; aborted before or during the delay raises `AbortError` (`Request aborted`). | `abortable_sleep` uses biased `tokio::select!`; `aborts_a_provider_requested_retry_delay`. |
| `retryProviderRequest` | `maxRetries` defaults to 0; retries only provider errors that are retryable; aborted signal wins over the provider error; each retry is a fresh request. | `retry_provider_request`; `retries_retryable_provider_errors_after_the_server_delay`, `defaults_to_zero_retries`, `does_not_retry_non_provider_errors`, `retry_future_is_send`. |

## Exclusions and deviations

- `AbortError.message` is `The operation was aborted` because `CancellationToken` has no reason payload; the TypeScript helper uses `Request aborted`. Traceability records this as a cross-module mapping, not a source discrepancy.
- `Date.parse` accepts HTTP dates and ECMAScript ISO 8601. V8 legacy formats (`"Jan 1 2999"`, `"1/1/2020"`) and HTTP dates before 1970 are not supported. Zone-less ISO date-times use the process-local zone, matching V8.
- Non-finite `retry-after` values: `parseFloat("Infinity")` is `Infinity`, so the default 60s cap rejects the delay like the source; with the cap disabled the Tokio duration conversion fails and the sleep is zero.
- Random jitter is drawn from `getrandom`; deterministic tests assert the 25% jitter band and capped base.
- No differential oracle: timers, wall clock, randomness, and abort scheduling require a JS event loop.
