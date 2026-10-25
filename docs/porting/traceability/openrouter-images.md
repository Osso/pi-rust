# openrouter-images traceability

## Q4 common-pin status

Superseded by the 2026-10-25 whole-module audit below. Corrected global-fetch oracle drives frozen SDK. Source abort text `Request was aborted.` restored after two RED behavioral tests and source SDK error proof (`core/error.mjs:67–69`). Historical ledger `/tmp/pi-rust-repin-delta-ai/ledger.md`. [Audit/proof](../ai-scripted-provider-oracles.md). Historical full differential claims below are superseded. No assertion narrowed; `request.fetch` is an approved Rust-only seam; unset must match frozen default fetch (`AGENTS.md`).

Source: `packages/ai/src/api/openrouter-images.ts`, `packages/ai/src/api/openrouter-images.lazy.ts`.
Rust: `crates/pi-ai/src/api/openrouter_images.rs`, `crates/pi-ai/src/api/openrouter_images_lazy.rs`.
Tests: `crates/pi-ai/tests/openrouter_images.rs` (13 behavioral), `crates/pi-ai/tests/differential_openrouter_images.rs` (10 recorded-fixture scenarios vs pinned TS; request method/URL/body/headers and `AssistantImages` compared).

## Representation
- `generateImages` → `generate_images(model, context, options) -> BoxFuture<'static, Result<AssistantImages, CallbackError>>`. Failures are encoded in the returned `AssistantImages` (`stopReason: "error" | "aborted"`), matching TypeScript; `Ok` is always returned.
- The frozen `openai@6.26.0` client behavior is reused from `api::openai_sdk` (`SdkClient`, `create_chat_completion`, `format_request_error`), not reimplemented. `SdkClientConfig.default_headers` receives `providerHeadersToRecord({ ...model.headers, ...options.headers })` (exact-key merge, explicit `null` suppressions dropped).
- `openrouterImagesApi()` → `openrouter_images_api() -> Arc<dyn ProviderImages>` wrapping `OpenRouterImagesApi`; Rust links the implementation directly (no dynamic import).
- Initial request body is built in source order (`model`, `messages`, `stream`, `modalities`), matching `JSON.stringify`. Image onPayload and nonstream SDK JSON are native `JsValue`; response captions retain exact UTF-16.
- `onPayload`/`onResponse` use the image-specific callback types in `ImagesRequestOptions` (TypeScript parameterizes `ProviderRequestOptions` over `ImagesModel`).
- Timeout: the SDK client default is 10 minutes; `timeoutMs` covers fetch acquisition/headers, not successful response-body reads. The old second whole-body timeout is removed. Source does not use pi's `maxRetryDelayMs`; retry requests increment the SDK attempt header.
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
- Pathological wrong-typed response shapes, scalar image URL/base64 carriers with unmatched units, fractional/negative retry counts, and SDK/runtime timer scheduling remain explicit gaps under the user-observable bar; no lossy conversion is used for human captions or callback JSON.
- The TypeScript SDK logs, `withResponse()` response metadata beyond status/headers, and `dangerouslyAllowBrowser` are not observable here.
- Current early/body-read abort cases preserve source SDK default `Request was aborted.` exactly; arbitrary reason handling uses the shared SDK abort path.
- Header names are lowercased by the SDK stand-in; JavaScript `Headers` does the same on the wire. The image oracle retains exact retry-count; host-version identity headers remain normalized, while shared SDK fixtures own that identity boundary.

## Discrepancies
- None.

## Q18 provider request integration (2026-10-22)

Image SDK client key, base URL and model/options header values are native; shared SDK request construction owns ByteString and URL conversion. Content sanitation sites remain source-specific and unchanged.

[Focused public-provider runtime proof](lossless-sdk-boundaries.md) records exact covered cases, revisions and remaining acceptance limits.

## Whole common-pin audit (2026-10-25)

All exports and branches in frozen 185-line module and 10-line lazy factory compared with native: key guard, header precedence, source sanitation, modalities, callback replacement/catch, SDK timeout/retry options, response ID/usage/cache arithmetic/cost, first-choice caption, image URL forms/regex, and abort/error catch. Deleted duplicate body timeout; no new dependency or fallback.

`differential_openrouter_images` now drives 19 request/response scenarios (0–13,15–19) plus native UTF-16 response/payload replacement. Genuine REDs: non-JSON 2xx produced the wrong SyntaxError instead of the source property error; native JSON rejected a valid unmatched caption; retry attempt stayed zero; successful slow body incorrectly timed out after headers. The widened image callback is mechanical shared-contract fallout in request.rs and its existing behavioral test. Callback string/object rejection now uses the source Error/JSON catch policy; empty API key never sends a request. Content-Type JSON/+json and 204/null decoding use the real SDK boundary, not unconditional JSON parse.

Proof artifacts: `/tmp/ai-api-images-{red,retry-red,timeout-red,green}.log`. The source method's unused maxRetryDelayMs is not forwarded. Any remaining SDK host/timer/numeric/pathological boundaries are the explicit gap row above, not full live-provider acceptance.

The timeout behavior fixture now rejects when the SDK-owned AbortSignal fires, as native fetch does. Its old fake ignored abort and therefore tested the deleted whole-body timeout rather than the frozen SDK; the exact timeout-message assertion is unchanged.

## Native callback catch audit (2026-10-25)

`differential_api_catches` compares full actual-source/native error events and final messages for eight APIs × four rejection values: unmatched-UTF16 string, raw Error-lookalike object, and genuine Errors with status/body or statusCode/body. All callbacks reject before HTTP (source records zero requests). Genuine REDs were empty native messages for raw rejections and discarded branded status/body fields. Anthropic keeps its literal Error.message/JSON catch; Mistral probes only statusCode with its own prefix/body truncation; Google/OpenAI/Azure/images adapt the original branded fields to the existing shared normalizer. No utility algorithm or dependency changed. Logs: `/tmp/ai-api-catches-{red,red-2,images-red,green}.log`.
