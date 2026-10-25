# Azure OpenAI Responses traceability

Frozen: `packages/ai/src/api/azure-openai-responses.ts` at [source lock](../source-lock.json),297 lines. Native: `crates/pi-ai/src/api/azure_openai_responses.rs`, shared SDK and [Responses algorithms](openai-responses-shared.md).

## Whole-module audit (2026-10-25)

| Source symbols / branches | Proof |
|---|---|
| `AzureOpenAIResponsesOptions`, `DEFAULT_AZURE_API_VERSION`, provider set | exact source request matrix; native scalar options with native URL/config values |
| `parseDeploymentNameMap`, `resolveDeploymentName`: comma split, trim, two-segment equals split, ignore missing segments, last duplicate wins, truthy option→map→model | `azure_options_match_frozen_empty_precedence_map_splits_and_sdk_retry`; unit map cases |
| `normalizeAzureBaseUrl`: trim/slash removal, invalid URL error, three Azure suffixes, four normalized root paths, query removal, custom path retention | original request/config cases and `azure_hosts_get_the_v1_base_path` |
| `buildDefaultBaseUrl`, `resolveAzureConfig`: truthy version option→env→v1; base option→env; truthy resource option→env; model URL; explicit missing error | original env/custom/resource cases; empty option precedence regression; native lossless config fixtures |
| `createClient`: model→caller headers, Azure key/version/base URL | full SDK request header/body/URL comparisons; Responses path is not a deployment endpoint in frozen SDK |
| `buildParams`: deployment/input/stream/cache/store, truthy maxTokens, temperature, plain tools, mapped/default/off reasoning | original12 scenarios plus five config/request/retry boundary cases; removed old-only sampling merge and output floor |
| `stream`: per-request deployment/output, key guard, payload callback, SDK options, response callback, start/shared stream, abort/unknown-error guard, terminal/catch scratch cleanup | exact source events/results including open tool failure; `differential_api_catches` preserves raw/native error branding, prefix/status/body |
| `streamSimple`: synchronous missing/empty key rejection, base options and reasoning clamps | `differential_simple_acquisition`; existing medium Simple case; [base options](simple-options.md) |
| lazy factory | [lazy proof](api-lazy.md) |

SDK request maxRetries is honored with exact attempt headers. pi's maxRetryDelayMs is not a frozen SDK request option and is ignored here. No source correction, dependency, fallback or deployment.

## Explicit gaps

- SDK runtime/OS-fault shapes, pathological wrong-typed JSON/numeric extremes, and unmatched units beyond existing opaque/schema carriers follow the user-observable boundary. Default SDK malformed-SSE console diagnostics remain an open shared-shim proof at this checkpoint, not an approved omission.
- Native partial snapshots and native callback ownership use the existing contracts. URL IDNA/runtime edge cases are not hunted.

Proof ledger: [AI API re-pin proof](../ai-api-repin-proof.md). Earlier [native SDK boundaries](lossless-sdk-boundaries.md) remain bounded historical evidence, not a workspace gate.
