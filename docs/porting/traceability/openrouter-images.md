# openrouter-images traceability

## Q4 common-pin status

Partial at remaining lossless interface/response boundaries. Corrected global-fetch oracle now drives frozen SDK and full current scenario matrix is GREEN, plus13 behavioral tests. Source abort text `Request was aborted.` restored after two RED behavioral tests and source SDK error proof (`core/error.mjs:67–69`). Ledger `/tmp/pi-rust-repin-delta-ai/ledger.md`, `openrouter_images-sdk626-abort.log` and `differential_openrouter_images-sdk626-abort.log`. Nonstream SDK response JSON and image onPayload still cross scalar Value; these unapproved gaps remain pending re-port. [Audit/proof](../ai-scripted-provider-oracles.md). Historical full differential claims below are superseded. No assertion narrowed; `request.fetch` is an approved Rust-only seam; unset must match frozen default fetch (`AGENTS.md`).

Source: `packages/ai/src/api/openrouter-images.ts`, `packages/ai/src/api/openrouter-images.lazy.ts`.
Rust: `crates/pi-ai/src/api/openrouter_images.rs`, `crates/pi-ai/src/api/openrouter_images_lazy.rs`.
Tests: `crates/pi-ai/tests/openrouter_images.rs` (13 behavioral), `crates/pi-ai/tests/differential_openrouter_images.rs` (10 recorded-fixture scenarios vs pinned TS; request method/URL/body/headers and `AssistantImages` compared).

## Representation
- `generateImages` → `generate_images(model, context, options) -> BoxFuture<'static, Result<AssistantImages, CallbackError>>`. Failures are encoded in the returned `AssistantImages` (`stopReason: "error" | "aborted"`), matching TypeScript; `Ok` is always returned.
- The frozen `openai@6.26.0` client behavior is reused from `api::openai_sdk` (`SdkClient`, `create_chat_completion`, `format_request_error`), not reimplemented. `SdkClientConfig.default_headers` receives `providerHeadersToRecord({ ...model.headers, ...options.headers })` (exact-key merge, explicit `null` suppressions dropped).
- `openrouterImagesApi()` → `openrouter_images_api() -> Arc<dyn ProviderImages>` wrapping `OpenRouterImagesApi`; Rust links the implementation directly (no dynamic import).
- Request body is built with `serde_json::Map` insertion order (`model`, `messages`, `stream`, `modalities`), matching `JSON.stringify`.
- `onPayload`/`onResponse` use the image-specific callback types in `ImagesRequestOptions` (TypeScript parameterizes `ProviderRequestOptions` over `ImagesModel`).
- Timeout: the SDK client default is 10 minutes; `timeoutMs` (including `0` meaning no timeout) races the request and maps expiry to the SDK's `Request timed out.` connection error, which the retry wrapper may retry.
- Data URLs are parsed with the source regex semantics: `data:<mime>;base64,<data>`, non-empty mime and payload.

## Symbols / branches → proof
| TS | Rust | Test |
|---|---|---|
| missing `apiKey` → `No API key for provider: X` | `request_images` first branch | `missing_api_key_returns_an_error_without_a_request` |
| `createClient` (baseURL, bearer auth, default headers, `dangerouslyAllowBrowser`) | `SdkClientConfig` + `merged_headers` | `returns_text_and_images_and_builds_the_sdk_request`, differential 0/1 |
| `buildParams` text/image parts, modalities | `build_params` | `returns_text...`, `sends_image_inputs_as_data_urls`, `image_only_models_request_image_modality_only`, differential 0/1/2 |
| `sanitizeSurrogates` | `utils::sanitize_unicode` (identity for Rust strings) | body fixture |
| `onPayload` replacement / throw | `request_images` callback branch | `on_payload_can_replace...` |
| SDK `maxRetries: 0` + `retryProviderRequest` | `ProviderRetryOptions` + `retry_provider_request` | `retries_transient_failures_when_requested` |
| `onResponse(status, headers)` before use | `run_generate_images` success branch | `on_payload_can_replace...` |
| `responseId` | `response_id` | success tests, differential 0 |
| `usage` parsing and cache-write rule | `parse_usage` | `usage_math_matches...`, `cache_write_larger...`, differential 2 |
| cost computation | `parse_usage` cost block | success + usage tests |
| `choices[0].message.content` non-empty text | `apply_response` | success test, differential 0 |
| `images[].image_url` string/object, `data:` filter, regex capture | `apply_response` + `parse_data_url` | success test (skips https, empty data, missing base64), differential 0 |
| abort signal before/during request → `Request aborted`, `stopReason: aborted` | `SdkClient` abort + `fail` | `aborted_signal_returns_aborted_without_a_request`, `abort_during_body_read_returns_aborted`, differential 3 |
| HTTP error normalization | `openai_sdk` + `format_request_error` | `http_errors_match_the_sdk_normalization`, differential 4-9 |
| timeout (`timeoutMs`, 10-minute default) | `timeout_duration` + `timeout_error` | `timeout_returns_the_sdk_timeout_message` |
| lazy API factory | `openrouter_images_api` | used by `images-models`/provider tests |

## Gaps
- `2xx` non-JSON body handling still needs frozen6.26 SDK proof; the historical6.40 discrepancy claim is retired, not an approved deviation.
- The TypeScript SDK logs, `withResponse()` response metadata beyond status/headers, and `dangerouslyAllowBrowser` are not observable here.
- Arbitrary signal.reason is unproven. Current early/body-read abort cases preserve source SDK default `Request was aborted.` exactly.
- Header names are lowercased by the SDK stand-in; JavaScript `Headers` does the same on the wire. Differential headers are compared after dropping `user-agent`/`x-stainless-*`, which are environment/SDK-internal.

## Discrepancies
- None.

## Q18 provider request integration (2026-10-22)

Image SDK client key, base URL and model/options header values are native; shared SDK request construction owns ByteString and URL conversion. Content sanitation sites remain source-specific and unchanged.

[Focused public-provider runtime proof](lossless-sdk-boundaries.md) records exact covered cases, revisions and remaining acceptance limits.
