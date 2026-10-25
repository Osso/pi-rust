# azure-openai-responses traceability

## Q4 common-pin status

Partial. Corrected global-fetch oracle now drives frozen SDK requests. SDK header6.26.0 restored; removed output floor and source-absent shared rawStopReason/strict-schema rewriting. Original9 recorded scenarios pass after shared incomplete-event handling correction (`responses-incomplete-cache-green.log`). Added actual SDK429→success retry scenario9: native retry header0 versus frozen1 RED (`azure-retry-red.log`); retry construction now supplies attempt count. All10 recorded scenarios pass at dbcc11de (`responses-retry-green.log`). Shared-module differentials still expose old grammar/custom-tool conversion and namespace drift; whole-module parity remains partial. [Exact audit/proof](../ai-scripted-provider-oracles.md). Historical recorded-fetch parity claims below are superseded. No assertion narrowed; `request.fetch` is an approved Rust-only seam; unset must match frozen default fetch (`AGENTS.md`).

Source: `packages/ai/src/api/azure-openai-responses.ts` and `azure-openai-responses.lazy.ts`.
Rust: `crates/pi-ai/src/api/azure_openai_responses.rs`.
Tests: module unit tests, `crates/pi-ai/tests/differential_azure_openai_responses.rs` (9 recorded-fetch scenarios).

## Symbols → proof
| TS | Rust | Test |
|---|---|---|
| `DEFAULT_AZURE_API_VERSION` | constant | 0 |
| `AZURE_TOOL_CALL_PROVIDERS` | constant | conversion |
| `OPENAI_RESPONSES_MIN_OUTPUT_TOKENS` | constant | 0 |
| `parseDeploymentNameMap` | `parse_deployment_name_map` | 2; unit test |
| `resolveDeploymentName` | `resolve_deployment_name` | 0-2 |
| `formatAzureOpenAIError` | `format_sdk_error`/`format_request_error` with `"Azure OpenAI API error"` | 6 |
| `AzureOpenAIResponsesOptions` | `AzureOpenAIResponsesOptions` | 1, 4 |
| `stream` / `streamSimple` | same + `AzureOpenAIResponsesApi` | all |
| `normalizeAzureBaseUrl` | `normalize_azure_base_url` | 0-2; unit test |
| `buildDefaultBaseUrl` | `build_default_base_url` | unit test |
| `resolveAzureConfig` | `resolve_azure_config` | 1-2 |
| `createClient` (api-key, api-version, headers) | `create_client` | all request comparisons |
| `buildParams` | `build_params` | 0-5 |
| strict mode default `true` | `build_params` | 3 |
| lazy wrapper | `azure_open_ai_responses_api` | `LazyApi` covered by `api_lazy` tests |

## Differential normalization
Same as `openai-responses`: timestamps, environment identity headers, and malformed-JSON text.

## Gaps
- Direct `streamSimple` API-key failure rejects acquisition; a lazy wrapper catches it asynchronously, matching frozen.
- The pinned Azure SDK does not add `/deployments/{model}` for `/responses`; the URL is `/openai/v1/responses?api-version=...` and the deployment name is only `body.model`.

## Discrepancies
- None.

## Q18 provider request integration (2026-10-22)

Azure endpoint/resource/API-version/deployment values and environment map parsing now use native units. URL normalization applies the shared WHATWG boundary, and SDK URL/header conversion propagates through existing failure handling. Grammar property names remain the existing scalar metadata contract.

[Focused public-provider runtime proof](lossless-sdk-boundaries.md) records exact covered cases, revisions and remaining acceptance limits.

## Direct Simple acquisition boundary (2026-10-25)

`differential_simple_acquisition` invokes actual frozen modules and native ProviderStreams for missing/empty API keys across six APIs. All twelve cases were genuine RED: native returned a stream instead of rejecting acquisition. Direct stream_simple now returns Result and adapters propagate it; lazy acquisition continues through its existing catch. Mechanical consumer changes preserve valid-stream assertions. The remaining module audit is separate from this bounded proof.
