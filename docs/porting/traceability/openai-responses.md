# OpenAI Responses API traceability

Frozen: `packages/ai/src/api/openai-responses.ts` at [source lock](../source-lock.json), 297 lines. Native: `crates/pi-ai/src/api/openai_responses.rs`; SDK transport: `openai_sdk.rs`; message/tool/event algorithms: [shared module](openai-responses-shared.md).

## Whole-module audit (2026-10-25)

| Source symbols / branches | Native representation | Behavioral proof |
|---|---|---|
| `OpenAIResponsesOptions`, `buildParams`: model/input/stream/cache/store, truthy maxTokens, temperature, serviceTier including null, function tools | native payload; only the three frozen option fields, no old toolChoice/sampling merge | `responses_options_match_frozen_selection_empty_session_and_sdk_retry`; existing21 scenarios |
| `hasHeader`, `getClientApiKey`: API key, caller Authorization/CF gateway auth, missing/blank key | native header/key checks; direct Simple returns Result | `response_auth_headers_and_service_tier_precedence_match_frozen`, twelve-case `differential_simple_acquisition` |
| `resolveCacheRetention`, `getCompat`, `getPromptCacheRetention` | explicit retention / scoped env / short default; nullish compat defaults; conditional24h | existing none/long/Copilot/proxy/session cases; native lossless registry compat fixtures |
| `createClient`: model→Copilot→nonempty session→caller headers | ordered header merge and shared SDK6.26 request path | exact requests in all source scenarios; empty session omission regression |
| `stream`: output initialization, payload replacement, SDK request options, response callback, start, shared stream, abort/unknown-stop guards, done, catch scratch cleanup | async producer; original callback branding and raw JSON kept | exact events/results; open-tool success/error/incomplete/arguments.done five-case fixture; `differential_api_catches` |
| `streamSimple`: synchronous auth, base selection, model reasoning clamp, standard clamp | Result before stream acquisition; lazy wrapper owns async catch | common-pin direct missing/empty auth matrix; simple high scenario; shared base-option differential |
| `getServiceTierCostMultiplier`, `applyServiceTierPricing`: flex0.5, gpt-5.5 priority2.5, other priority2, default1; response tier wins | same cost arithmetic and native shared pricing callback | unit pricing table plus eight actual-source request/response-tier cases |
| lazy factory | native `LazyApi` factory | [lazy EOF/acquisition/uncaught proof](api-lazy.md) |

Genuine REDs removed old-only toolChoice/sampling request fields, empty session headers, dropped valid null serviceTier, and capped SDK retries with pi's unrelated maxRetryDelayMs. SDK retry attempt counts remain exact. Callback catches no longer erase raw rejection strings/objects or branded status/body fields. No frozen logic correction or new dependency.

## Explicit gaps

- SDK runtime/OS-fault shapes, pathological wrong-typed JSON/numeric extremes, and unmatched units beyond existing opaque/schema carriers follow the one-line user-observable boundary; no new representation is claimed for them. Default SDK malformed-SSE console diagnostics are still an open shared-shim proof at this checkpoint, not an approved omission.
- Event partials are immutable native push-time snapshots (approved); native callbacks use the existing typed contracts. Unset Rust-only fetch injection uses the default HTTP client.

Proof commands and exact revisions/logs belong in [AI API re-pin proof](../ai-api-repin-proof.md). Historical narrow ingress/error proofs remain [linked](lossless-provider-ingress.md), [linked](lossless-provider-errors.md).
