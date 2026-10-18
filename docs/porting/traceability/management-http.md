# coding-agent management-http traceability

Pinned source: `packages/coding-agent/src/utils/management-http.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Differential test skipped: the source calls the global `fetch`; the Rust port defines an explicit transport trait. Retry semantics are proven with a programmable mock and a real local HTTP server in `crates/pi-coding-agent/tests/management_http.rs`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `management-http.ts:27` | Retryable statuses: 408, 425, 429, 500, 502, 503, 504 | `RETRYABLE_STATUS_CODES`; focused status tests |
| `management-http.ts:29-36` | Options: max retries, status retries, per-attempt timeout | `utils::management_http::FetchRetryOptions`; focused tests |
| `management-http.ts:54-57` | Default two retries; clamp and floor supplied values | `Option<usize>` typed boundary, default 2; focused default retry test |
| `management-http.ts:58-66` | Overall timeout budget shared by attempts; caller signal combined | `deadline` computed once and remaining budget passed per attempt; focused shared-budget test |
| `management-http.ts:68-69` | Caller cancellation is terminal before each attempt | `CancellationToken` check; focused pre-cancelled test |
| `management-http.ts:72-80` | Return non-retryable responses; retry transient responses | `fetch_with_retry`; focused status tests |
| `management-http.ts:81-90` | Retry transport failures; return the last failure after exhaustion | Focused transport tests |
| `management-http.ts:49-92` | Real transport | `UreqFetcher`; focused local-server retry test |

## Discrepancies

- `Response` is replaced by `HttpResponse { status, body }`; bodies are read eagerly.
- In-flight caller cancellation is not observed until the blocking `ureq` call returns. Cancellation is checked between attempts; per-attempt timeout bounds the request.
- Non-finite and fractional `maxRetries` values are excluded by the typed `Option<usize>` boundary.
- `AbortError` conditions are represented by `FetchError::{Cancelled, Timeout, Transport}`.

## Proof inventory

- Focused cases: 10 (mock retry matrix plus a real two-request server).
