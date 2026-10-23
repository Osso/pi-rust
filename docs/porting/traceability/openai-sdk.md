# openai-sdk traceability

## Q4 common-pin SDK identity

Frozen installed `node_modules/openai/package.json:3` and `core/error.mjs:67–69` identify SDK6.26.0 and APIUserAbortError text `Request was aborted.`. Native SDK headers now use6.26.0, with client names `OpenAI/JS` versus Azure's `AzureOpenAI/JS`, and retry-layer abort formatting matches that exact message. Unmasked Azure whole-provider differential reproduced the wrong client name, then passed all11 scenarios after selecting the configured SDK class name (`azure-sdk-name-green.log`). Three corrected actual-SDK differentials proved RED version6.40→6.26, then advanced to independent body/shape drifts; OpenRouter Images now GREEN13 behavioral +1 whole differential after two abort REDs. Revision `6ec0609f` + identity slice, `/tmp/pi-rust-repin-delta-ai/ledger.md`.

Approved Rust-only `options.request.fetch` seam is recorded in AGENTS.md; absence must preserve frozen default-fetch behavior. It is not a frozen option, source oracle injection, or fallback. Shared HTTP response JSON is still scalar in the nonstreaming SDK path and requires lossless migration; no whole-SDK parity claim.

Q18 human-error representation and bounded proof: [lossless provider errors](lossless-provider-errors.md). Earlier counts are historical, not rerun evidence.

Q18 ingress representation and current bounded proof: [lossless provider ingress](lossless-provider-ingress.md). Earlier suite counts below are historical, not new-pin rerun evidence.

Source: private behavior of frozen `openai@6.26.0` (`node_modules/openai/src/client.ts`, `azure.ts`, `core/streaming.ts`, `core/error.ts`) reproduced for the four OpenAI adapters.
Rust: `crates/pi-ai/src/api/openai_sdk.rs`.
Tests: covered indirectly by `crates/pi-ai/tests/differential_openai_responses.rs` (20 scenarios), `differential_azure_openai_responses.rs` (9), `differential_openai_completions.rs` (35). Requests, headers, bodies, SSE decoding, HTTP errors, and retries are compared against the real SDK.

## Shared SDK reason/deadline boundary

`api/stainless_fetch.rs` reproduces the identical pinned OpenAI6.26/Anthropic0.91 fetchWithTimeout/castToError branches used by both adapters. Each fetch gets a new controller signal, with a persistent caller listener that deliberately aborts it without forwarding custom reasons. Its header timer uses Node setTimeout delay normalization, not AbortSignal.timeout's uint32 validation, and ends on fetch completion. Supplied Error/DOMException rejections preserve branding for AbortError/message/cause timeout detection; non-Errors undergo the SDK's JSON-object versus primitive conversion before classification. A non-callable cause.toString throws the source TypeError instead of becoming Connection error.

`differential_stainless_abort` resolves the actual SDK packages from the frozen provider path. RED `stainless-abort-reasons-red.log` (missing signal), GREEN `stainless-abort-reasons-green-1.log`, expanded GREEN `stainless-abort-reasons-expanded.log`: 19 cases ×2 SDKs, including header deadline, caller custom reason, missing caller, successful fetch identity/lifetime, cleared timer, pre-abort/no fetch, branded and lookalike errors, raw null/text, cause-string and cause-conversion failure. Native Error/cause UTF-16 is never stringified lossily. Body stream rejections, recursive branded causes, and provider event/result coverage of these new cases remain partial; this is a bounded SDK request proof, not whole-provider completion.

## Representation
- `SdkClient` builds `HttpRequest` values and sends them through `Fetch` (`options.request.fetch` or `http::default_fetch()`).
- `SseJsonStream` decodes `\n\n` / `\r\r` / `\r\n\r\n`-delimited SSE chunks, joins multi-line `data:` fields, stops at `[DONE]`, and maps `data.error` to `ProviderError` exactly like `new APIError(undefined, data.error, ...)`.
- HTTP failures reproduce `APIError.generate` message construction and `normalizeProviderError` field probing. `ProviderError` was extended with `body` (parsed SDK inner error text) and `inner_error` so `formatProviderError` produces the same status/body output as TypeScript.
- `X-Stainless-Runtime-Version` is emitted as `unknown`; Rust has no Node `process.version`. Differential tests scope normalization to environment-derived `x-stainless-os/arch/runtime/runtime-version` headers; deterministic SDK user-agent strings compare exactly. [Normalizer audit](../ai-scripted-provider-oracles.md#provider-oracle-normalizer-audit-2026-10-22).

## Symbols → proof
| Behavior | Rust | Test |
|---|---|---|
| URL join (`buildURL`, trailing-slash rule) | `SdkClient::build_url` | request URL in all scenarios |
| Azure `api-version` default query | `build_url` + `AzureClientConfig` | azure 0-8 |
| Header assembly order/override/null suppression | `SdkClient::build_headers` | request headers in all scenarios |
| `Authorization: Bearer` / Azure `api-key` | `build_headers` | responses/completions vs azure |
| JSON body encoding (`FallbackEncoder`) | `send` (`serde_json::to_vec`) | request body in all scenarios |
| Non-streaming `create(..., { stream: false })` JSON response | `SdkClient::create_chat_completion` (`send_json`) | `differential_openrouter_images` |
| `APIError.generate` status/body message | `error_message`, `api_error_detail` | responses 12-13, completions 22-23 |
| SSE chunk splitting and line decoding | `find_double_newline_index`, `decode_sse_segment` | all stream scenarios (7-char chunk splits) |
| `[DONE]` handling | `SseJsonStream::next_event` | all stream scenarios |
| `data.error` stream error | `next_event` | covered by error fixtures |
| HTTP retry policy (SDK `maxRetries: 0`, provider retry wrapper) | `utils::provider_retry` | responses 14, completions 23 |
| Abort swallows stream errors | `fill_events` | responses 15, azure 8, completions 24 |
| Error formatting | `format_sdk_error`, `format_request_error` | HTTP error scenarios |

## Gaps
- SDK log events, `tee()`, pagination, multipart/binary bodies, websocket transport, and `thread.*` event synthesis are not used by these modules.
- The SDK's own retry loop is always disabled (`maxRetries: 0`); retry behavior is the already-ported `retryProviderRequest`.
- Environment identity headers cannot be byte-identical to Node and are normalized in differential tests.

## Discrepancies
- Azure `AzureOpenAI.buildRequest` only prefixes `/deployments/{model}` for a fixed endpoint set that does **not** include `/responses`; the port reproduces that behavior (deployment name is sent as `body.model` only).
- TypeScript stream events hold live `partial` references that mutate after `push`; Rust emits value snapshots. Differential oracles snapshot at `push` time, which is the observable value for a consumer reading each event.

## Q18 provider request integration (2026-10-22)

`SdkClientConfig` keeps API key, base URL, Azure API version and default-header values as `JsString`. `send_raw` converts URL/header values through shared `request_url`/`header_value` while constructing `HttpRequest`, returning their `CallbackError` through `ProviderRequestError::Other`.

[Focused public-provider runtime proof](lossless-sdk-boundaries.md) records exact covered cases, revisions and remaining acceptance limits.
