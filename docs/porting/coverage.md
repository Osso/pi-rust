# Port coverage matrix

Source: `packages/` at the commit in [source-lock.json](source-lock.json). Non-test `src/**/*.ts` only (671 files, 164,076 lines).

Status: `todo` not started; `partial` some exports ported (see traceability); `done` whole module ported with tests; `gap` approved exclusion (reason in Notes).

Level = import-graph depth (0 = no internal imports); port lower levels first.

## telemetry (3 files, 596 lines)

| Source | Lines | Level | Status | Rust | Notes |
|---|---:|---:|---|---|---|
| `telemetry/src/index.ts` | 357 | 0 | done | `crates/pi-telemetry/src/lib.rs` | Runtime contracts, schema data, and typed starter ported; TypeScript conditional type inference helpers are an explicit gap (see traceability). |
| `telemetry/src/memory.ts` | 219 | 0 | done | `crates/pi-telemetry/src/memory.rs` | In-memory recording ported with detached snapshots; JavaScript proxy/allocation passivity branches are not representable (see traceability). |
| `telemetry/src/noop.ts` | 20 | 0 | done | `crates/pi-telemetry/src/noop.rs` | Inert context ported; JavaScript object freezing is not applicable. |

## protocol (8 files, 1236 lines)

| Source | Lines | Level | Status | Rust | Notes |
|---|---:|---:|---|---|---|
| `protocol/src/cbor/options.ts` | 52 | 0 | done | `crates/pi-protocol/src/cbor/options.rs` | Limits/defaults/error types; differential limit fixtures |
| `protocol/src/framing.ts` | 165 | 0 | done | `crates/pi-protocol/src/framing.rs` | Behavioral and differential proof |
| `protocol/src/schemas.ts` | 450 | 0 | done | `crates/pi-protocol/src/schemas.rs` + `src/schemas/*` | All exports, including transcripts, snapshots, results, server messages |
| `protocol/src/cbor/decoder.ts` | 168 | 1 | done | `crates/pi-protocol/src/cbor/decoder.rs` | TS test port + byte-level differential vectors |
| `protocol/src/cbor/encoder.ts` | 216 | 1 | done | `crates/pi-protocol/src/cbor/encoder.rs` | TS test port + byte-level differential vectors |
| `protocol/src/cbor/index.ts` | 9 | 2 | done | `crates/pi-protocol/src/cbor/mod.rs` | Public re-exports |
| `protocol/src/codec.ts` | 172 | 3 | done | `crates/pi-protocol/src/codec.rs` + `src/codec/*` | TS protocol test port; parse/serialize round-trips |
| `protocol/src/index.ts` | 4 | 4 | done | `crates/pi-protocol/src/lib.rs` | Package-root re-exports |

## ai (177 files, 23710 lines)

| Source | Lines | Level | Status | Rust | Notes |
|---|---:|---:|---|---|---|
| `ai/src/api/cloudflare.ts` | 15 | 0 | done | `crates/pi-ai/src/api/cloudflare.rs` | Four endpoint constants; exact differential. |
| `ai/src/api/openai-prompt-cache.ts` | 8 | 0 | done | `crates/pi-ai/src/api/openai_prompt_cache.rs` | Code-point clamp at 64; differential. |
| `ai/src/auth/oauth/device-code.ts` | 98 | 0 | done | `crates/pi-ai/src/auth/oauth/device_code.rs` | RFC 8628 polling with deadline, slow_down and cancellation; Tokio tests. |
| `ai/src/auth/oauth/oauth-page.ts` | 109 | 0 | done | `crates/pi-ai/src/auth/oauth/oauth_page.rs` | Success/error HTML rendering with escaping; byte-exact differential. |
| `ai/src/auth/oauth/pkce.ts` | 34 | 0 | done | `crates/pi-ai/src/auth/oauth/pkce.rs` | Base64url verifier and SHA-256 challenge; differential challenge check. |
| `ai/src/session-resources.ts` | 24 | 0 | done | `crates/pi-ai/src/session_resources.rs` | Global cleanup registry with unregister and aggregated errors; marker-isolated tests. |
| `ai/src/utils/abort-signals.ts` | 41 | 0 | done | `crates/pi-ai/src/utils/abort_signals.rs` | Child-token combination with bridge tasks; Tokio tests. |
| `ai/src/utils/abort.ts` | 50 | 0 | done | `crates/pi-ai/src/utils/abort.rs` | Abort race and operation signal over `CancellationToken`; Tokio tests. |
| `ai/src/utils/diagnostics.ts` | 45 | 0 | done | `crates/pi-ai/src/utils/diagnostics.rs` | Thrown-value formatting, error extraction, diagnostic creation/append, serde wire shape. |
| `ai/src/utils/error-body.ts` | 149 | 0 | done | `crates/pi-ai/src/utils/error_body.rs` | Provider error status/body normalization, formatting, truncation; differential. |
| `ai/src/utils/hash.ts` | 13 | 0 | done | `crates/pi-ai/src/utils/hash.rs` | UTF-16 code-unit short hash, bit-for-bit differential. |
| `ai/src/utils/json-parse.ts` | 124 | 0 | done | `crates/pi-ai/src/utils/json_parse.rs` | JSON repair + `partial-json@0.1.7` streaming parser; prefix/malformed differential corpus. |
| `ai/src/utils/provider-retry.ts` | 125 | 0 | done | `crates/pi-ai/src/utils/provider_retry.rs` | SDK retry policy, server delays, abortable sleep; Tokio tests. |
| `ai/src/utils/sanitize-unicode.ts` | 25 | 0 | done | `crates/pi-ai/src/utils/sanitize_unicode.rs` | Surrogate repair over UTF-16 units plus typed identity API; differential. |
| `ai/src/utils/typebox-helpers.ts` | 24 | 0 | done | `crates/pi-ai/src/utils/typebox_helpers.rs` | `StringEnum` builder plus explicit TypeBox-origin newtype ([traceability](traceability/typebox-helpers.md)). |
| `ai/src/utils/uuid.ts` | 48 | 0 | done | `crates/pi-ai/src/utils/uuid.rs` | RFC 9562 UUIDv7 with monotonic sequence; TS vectors + structural differential. |
| `ai/src/api/anthropic-messages.ts` | 1366 | 1 | done | `crates/pi-ai/src/api/anthropic_messages.rs` + `api/anthropic_sdk.rs` | Messages conversion, SDK request construction (URL/headers/body/status errors/retry), SSE state machine, thinking/cache/tool compat; 27 behavioral tests + 41 recorded-fixture differential scenarios. |
| `ai/src/api/azure-openai-responses.ts` | 330 | 1 | done | `crates/pi-ai/src/api/azure_openai_responses.rs` | Deployment/env/base-URL resolution, api-key auth, strict tools, reasoning; 9 recorded-fetch differential scenarios. |
| `ai/src/api/bedrock-converse-stream.ts` | 1190 | 1 | done | `crates/pi-ai/src/api/bedrock_converse_stream.rs` | Full stream/streamSimple, SigV4, event-stream framing, credential/profile chain, retries, custom headers; 16 behavioral + 18-scenario differential oracle |
| `ai/src/api/constrained-sampling.ts` | 277 | 1 | done | `crates/pi-ai/src/api/constrained_sampling.rs` | Strict-schema conversion, grammar sampling, append-only JSON deltas; differential 47 requests. |
| `ai/src/api/github-copilot-headers.ts` | 37 | 1 | done | `crates/pi-ai/src/api/github_copilot_headers.rs` | Initiator/vision/header builders; differential 16 requests. |
| `ai/src/api/google-generative-ai.ts` | 521 | 1 | done | `crates/pi-ai/src/api/google_generative_ai.rs` | stream/streamSimple, thinking config, tool/stop handling, private `@google/genai` stand-in; 45 differential scenarios + behavioral tests. |
| `ai/src/api/google-shared.ts` | 419 | 1 | done | `crates/pi-ai/src/api/google_shared.rs` | Message/tool conversion, thought signatures, stop reasons, Google retry policy; 30 ported tests + differential request bodies. |
| `ai/src/api/google-vertex.ts` | 596 | 1 | done | `crates/pi-ai/src/api/google_vertex.rs` | stream/streamSimple, Vertex URL/project/location rules, API-key vs ADC, service-account JWT + authorized-user token exchange; 21 differential scenarios + behavioral tests. |
| `ai/src/api/lazy.ts` | 98 | 1 | done | `crates/pi-ai/src/api/lazy.rs` | lazy stream/api, setup errors, declared deferred capabilities; differential setup-error |
| `ai/src/api/mistral-conversations.ts` | 931 | 1 | done | `crates/pi-ai/src/api/mistral_conversations.rs` | Native chat stream, wire remapping, SSE, reasoning modes, tool-call ID hashing; differential 43 scenarios (requests, events, messages). |
| `ai/src/api/openai-codex-responses.ts` | 1727 | 1 | todo |  |  |
| `ai/src/api/openai-completions.ts` | 1577 | 1 | done | `crates/pi-ai/src/api/openai_completions.rs` | Compat detection, message/tool conversion, all thinking formats, cache control, routing; 9 behavioral + 35 recorded-fetch differential scenarios. |
| `ai/src/api/openai-responses-shared.ts` | 983 | 1 | done | `crates/pi-ai/src/api/openai_responses_shared.rs` | Message/tool conversion, citation normalization, Responses SSE state machine; 19 behavioral + 31 differential scenarios. |
| `ai/src/api/openai-responses.ts` | 372 | 1 | done | `crates/pi-ai/src/api/openai_responses.rs` | Params, compat, cache/session affinity, service-tier pricing; 20 recorded-fetch differential scenarios. |
| `ai/src/api/pi-messages.ts` | 433 | 1 | done | `crates/pi-ai/src/api/pi_messages.rs` | SSE assistant-event protocol, rewrite diagnostics, response errors; differential 13 scenarios. |
| `ai/src/api/simple-options.ts` | 97 | 1 | done | `crates/pi-ai/src/api/simple_options.rs` | Context clamp, base options, reasoning clamps, thinking budgets; unit + streamSimple differential. |
| `ai/src/api/transform-messages.ts` | 228 | 1 | done | `crates/pi-ai/src/api/transform_messages.rs` | Cross-model thinking/tool-call normalization, image downgrade, synthetic tool results; injected clock; differential 18 scenarios. |
| `ai/src/auth/context.ts` | 45 | 1 | done | `crates/pi-ai/src/auth/context.rs` | default env/file context; env branch not unit-tested (process-global) |
| `ai/src/auth/credential-store.ts` | 67 | 1 | done | `crates/pi-ai/src/auth/credential_store.rs` | serialized modify/delete, abort-before-run, insertion-order list |
| `ai/src/auth/resolve.ts` | 205 | 1 | done | `crates/pi-ai/src/auth/resolve.rs` | stored/ambient resolution, locked OAuth refresh, ModelsError codes; differential auth cases |
| `ai/src/auth/types.ts` | 240 | 1 | done | `crates/pi-ai/src/auth/types.rs` | credentials, traits, prompts/events; OAuth extra keys preserved |
| `ai/src/models-store.ts` | 45 | 1 | done | `crates/pi-ai/src/models_store.rs` | clone semantics, cancellation, timestamp wire shape |
| `ai/src/models.ts` | 953 | 1 | done | `crates/pi-ai/src/models.rs` | providers/models, refresh generations+publication chains, auth application, login/logout, deferred, cost/thinking helpers; differential cost/thinking/refresh/apply-auth |
| `ai/src/types.ts` | 846 | 1 | partial | `pi-ai/src/types.rs` | data contracts done; option/callback interfaces → provider runtime slice |
| `ai/src/utils/deferred-tools.ts` | 39 | 1 | done | `crates/pi-ai/src/utils/deferred_tools.rs` | Immediate/deferred split with insertion order; differential 6 requests. |
| `ai/src/utils/estimate.ts` | 143 | 1 | done | `crates/pi-ai/src/utils/estimate.rs` | Context/message/tool estimation, UTF-16 lengths, added-tool accounting; differential 10 requests. |
| `ai/src/utils/event-stream.ts` | 110 | 1 | done | `pi-ai/src/utils/event_stream.rs` | differential |
| `ai/src/utils/headers.ts` | 18 | 1 | done | `crates/pi-ai/src/utils/headers.rs` | `Headers.entries()` normalization and provider header filtering; differential 7 requests. |
| `ai/src/utils/node-http-proxy.ts` | 112 | 1 | done | `crates/pi-ai/src/utils/node_http_proxy.rs` | Proxy selection plus `Fetch` wrapper; differential over target/env matrix (see traceability). |
| `ai/src/utils/provider-env.ts` | 52 | 1 | partial | `crates/pi-ai/src/utils/provider_env.rs` | Scoped → process env precedence; Bun `/proc/self/environ` fallback not applicable to native Rust (see traceability). |
| `ai/src/api/anthropic-messages.lazy.ts` | 4 | 2 | done | `crates/pi-ai/src/api/anthropic_messages_lazy.rs` | Lazy loader with no deferred capabilities; behavioral test. |
| `ai/src/api/azure-openai-responses.lazy.ts` | 4 | 2 | done | `crates/pi-ai/src/api/azure_openai_responses.rs` | `azure_open_ai_responses_api()` lazy adapter. |
| `ai/src/api/bedrock-converse-stream.lazy.ts` | 30 | 2 | done | `crates/pi-ai/src/bedrock_provider.rs` | `bedrock_converse_stream_api` + override seam; lazy test |
| `ai/src/api/cloudflare-gateway-binding.ts` | 192 | 2 | done | `crates/pi-ai/src/api/cloudflare_gateway_binding.rs` | Workers AI binding transport: prefix normalization, provider/endpoint split, header stripping, signal passthrough; differential 11 scenarios. |
| `ai/src/api/google-generative-ai.lazy.ts` | 4 | 2 | done | `crates/pi-ai/src/api/google_generative_ai.rs` | `google_generative_ai_api()` lazy wrapper. |
| `ai/src/api/google-vertex.lazy.ts` | 4 | 2 | done | `crates/pi-ai/src/api/google_vertex.rs` | `google_vertex_api()` lazy wrapper. |
| `ai/src/api/mistral-conversations.lazy.ts` | 4 | 2 | done | `crates/pi-ai/src/api/mistral_conversations.rs` | `mistral_conversations_api()` wraps `MistralConversationsApi` in `LazyApi`. |
| `ai/src/api/openai-codex-responses.lazy.ts` | 4 | 2 | todo |  |  |
| `ai/src/api/openai-completions.lazy.ts` | 4 | 2 | done | `crates/pi-ai/src/api/openai_completions.rs` | `open_ai_completions_api()` lazy adapter. |
| `ai/src/api/openai-responses.lazy.ts` | 4 | 2 | done | `crates/pi-ai/src/api/openai_responses.rs` | `open_ai_responses_api()` lazy adapter. |
| `ai/src/api/openrouter-images.ts` | 196 | 2 | done | `crates/pi-ai/src/api/openrouter_images.rs` | Request building, usage/image parsing, SDK error mapping, timeout; recorded-fixture differential (see traceability). |
| `ai/src/api/pi-messages.lazy.ts` | 4 | 2 | done | `crates/pi-ai/src/api/pi_messages.rs` | `pi_messages_api()` wraps `PiMessagesApi` in `LazyApi`. |
| `ai/src/auth/helpers.ts` | 59 | 2 | done | `crates/pi-ai/src/auth/helpers.rs` | env api-key auth and lazy OAuth loading |
| `ai/src/auth/oauth/anthropic.ts` | 364 | 2 | done | `crates/pi-ai/src/auth/oauth/anthropic.rs` | manual-code/callback race, localhost:53692 server, JSON exchange, refresh skew; differential manual + callback + refresh. |
| `ai/src/auth/oauth/kimi-coding.ts` | 310 | 2 | done | `crates/pi-ai/src/auth/oauth/kimi_coding.rs` | RFC 8628 device flow, host overrides, retry/unauthorized refresh; differential device + refresh. |
| `ai/src/auth/oauth/load.ts` | 68 | 2 | done | `crates/pi-ai/src/auth/oauth/load.rs` | Bundled-loader registry with direct-construction fallback; github-copilot fallback gap. |
| `ai/src/auth/oauth/openai-codex.ts` | 544 | 2 | done | `crates/pi-ai/src/auth/oauth/openai_codex.rs` | select prompt, device grant, localhost:1455 callback, JWT accountId, form exchange/refresh; differential device + refresh. |
| `ai/src/auth/oauth/openrouter.ts` | 311 | 2 | done | `crates/pi-ai/src/auth/oauth/openrouter.rs` | ephemeral PKCE callback, one-shot claim, key exchange, manual code, identity refresh; differential manual + callback. |
| `ai/src/auth/oauth/xai.ts` | 239 | 2 | done | `crates/pi-ai/src/auth/oauth/xai.rs` | https-only verification URIs, device polling, refresh-token preservation, one-hour default; differential device + refresh. |
| `ai/src/bedrock-provider.ts` | 6 | 2 | done | `crates/pi-ai/src/bedrock_provider.rs` | `bedrock_provider_module`; provider module test |
| `ai/src/compat/extension-oauth-types.ts` | 45 | 2 | done | `crates/pi-ai/src/compat/extension_oauth_types.rs` | Type-only extension callback surface and credential alias. |
| `ai/src/env-api-keys.ts` | 187 | 2 | done | `crates/pi-ai/src/env_api_keys.rs` | Provider env map, Anthropic token exclusion, Vertex ADC cache, and Bedrock ambient sources; differential over provider/env matrix (see traceability). |
| `ai/src/image-models.generated.ts` | 639 | 2 | done | `crates/pi-ai/data/image-models.json`, `crates/pi-ai/src/image_models_generated.rs` | Embedded TS-generated JSON with identical values and order; differential full catalog. |
| `ai/src/images-api-registry.ts` | 53 | 2 | done | `crates/pi-ai/src/images_api_registry.rs` | API registration/lookup and api-mismatch wrapper; `sourceId` excluded as unobservable (see traceability). |
| `ai/src/images-models.ts` | 275 | 2 | done | `crates/pi-ai/src/images_models.rs` | Providers, auth merge, dynamic refresh with in-flight dedupe; differential for registry/auth merge. `builtinImagesModels` gap tracked under providers/all. |
| `ai/src/model-catalog.ts` | 27 | 2 | done | `crates/pi-ai/src/model_catalog.rs` | Group flattening; `provider` parameter is type-only; differential 4 requests. |
| `ai/src/model-types.ts` | 273 | 2 | done | `crates/pi-ai/src/types.rs` | Type-only module; all interfaces mapped to existing `types.rs` items, no new code. |
| `ai/src/providers/cloudflare-auth.ts` | 103 | 2 | done | `crates/pi-ai/src/providers/cloudflare_auth.rs` | Per-field credential/env merge, workers vs gateway headers; differential auth + login flows. |
| `ai/src/providers/cloudflare-stream.ts` | 28 | 2 | done | `crates/pi-ai/src/providers/cloudflare_stream.rs` | Endpoint placeholder materialization; behavioral + differential. |
| `ai/src/providers/faux.ts` | 708 | 2 | done | `crates/pi-ai/src/providers/faux.rs` | deterministic faux streams, usage/cache estimation, abort, deferred; differential fixed-chunk scenarios |
| `ai/src/providers/radius-config.ts` | 96 | 2 | done | `crates/pi-ai/src/providers/radius_config.rs` | URL normalization, config sanitization, model projection, bearer config load; differential. |
| `ai/src/utils/overflow.ts` | 181 | 2 | done | `crates/pi-ai/src/utils/overflow.rs` | 26 overflow patterns, non-overflow exclusions, silent and length-stop overflow; differential 40 checks + pattern matrix. |
| `ai/src/utils/retry.ts` | 253 | 2 | done | `crates/pi-ai/src/utils/retry.rs` | Exact classifier plus async backoff/cancellation/callbacks; differential 45 classifications + 10 scripted loops. |
| `ai/src/utils/text.ts` | 12 | 2 | done | `crates/pi-ai/src/utils/text.rs` | `contentText` over assistant and tool-result blocks; differential 6 requests. |
| `ai/src/utils/validation.ts` | 350 | 2 | done | `crates/pi-ai/src/utils/validation.rs` | Full hand-written TypeBox-compatible validator, coercions, error messages; 25 behavioral + 67 differential cases ([traceability](traceability/validation.md)). |
| `ai/src/api/openrouter-images.lazy.ts` | 10 | 3 | done | `crates/pi-ai/src/api/openrouter_images_lazy.rs` | Direct-link factory replacing the dynamic import. |
| `ai/src/auth/oauth/radius.ts` | 403 | 3 | done | `crates/pi-ai/src/auth/oauth/radius.rs` | discovery, localhost:1456 browser callback, device grant, gateway token/refresh; differential device + browser callback + refresh. |
| `ai/src/image-models.ts` | 42 | 3 | done | `crates/pi-ai/src/image_models.rs` | Lookup/list APIs; `undefined` modeled as `Option`/empty slice (see traceability). |
| `ai/src/images.ts` | 21 | 3 | done | `crates/pi-ai/src/images.rs` | Builtin registration on first use, registry dispatch, unknown-api error. |
| `ai/src/index.ts` | 47 | 3 | todo |  |  |
| `ai/src/legacy-api-aliases.ts` | 108 | 3 | todo |  |  |
| `ai/src/oauth.ts` | 10 | 3 | done | `crates/pi-ai/src/oauth.rs` | Type-only compatibility re-exports. |
| `ai/src/providers/amazon-bedrock.models.ts` | 8 | 3 | done | `crates/pi-ai/src/providers/models_data.rs` | Generated `flattenModelCatalog` wrapper; embedded identical JSON; differential compares the full catalog and order. |
| `ai/src/providers/ant-ling.models.ts` | 8 | 3 | done | `crates/pi-ai/src/providers/models_data.rs` | Generated `flattenModelCatalog` wrapper; embedded identical JSON; differential compares the full catalog and order. |
| `ai/src/providers/anthropic.models.ts` | 8 | 3 | done | `crates/pi-ai/src/providers/models_data.rs` | Generated `flattenModelCatalog` wrapper; embedded identical JSON; differential compares the full catalog and order. |
| `ai/src/providers/azure-openai-responses.models.ts` | 8 | 3 | done | `crates/pi-ai/src/providers/models_data.rs` | Generated `flattenModelCatalog` wrapper; embedded identical JSON; differential compares the full catalog and order. |
| `ai/src/providers/baseten.models.ts` | 8 | 3 | done | `crates/pi-ai/src/providers/models_data.rs` | Generated `flattenModelCatalog` wrapper; embedded identical JSON; differential compares the full catalog and order. |
| `ai/src/providers/cerebras.models.ts` | 8 | 3 | done | `crates/pi-ai/src/providers/models_data.rs` | Generated `flattenModelCatalog` wrapper; embedded identical JSON; differential compares the full catalog and order. |
| `ai/src/providers/cloudflare-ai-gateway.models.ts` | 8 | 3 | done | `crates/pi-ai/src/providers/models_data.rs` | Generated `flattenModelCatalog` wrapper; embedded identical JSON; differential compares the full catalog and order. |
| `ai/src/providers/cloudflare-workers-ai.models.ts` | 8 | 3 | done | `crates/pi-ai/src/providers/models_data.rs` | Generated `flattenModelCatalog` wrapper; embedded identical JSON; differential compares the full catalog and order. |
| `ai/src/providers/deepseek.models.ts` | 8 | 3 | done | `crates/pi-ai/src/providers/models_data.rs` | Generated `flattenModelCatalog` wrapper; embedded identical JSON; differential compares the full catalog and order. |
| `ai/src/providers/fireworks.models.ts` | 8 | 3 | done | `crates/pi-ai/src/providers/models_data.rs` | Generated `flattenModelCatalog` wrapper; embedded identical JSON; differential compares the full catalog and order. |
| `ai/src/providers/github-copilot.models.ts` | 8 | 3 | done | `crates/pi-ai/src/providers/models_data.rs` | Generated `flattenModelCatalog` wrapper; embedded identical JSON; differential compares the full catalog and order. |
| `ai/src/providers/google-vertex.models.ts` | 8 | 3 | done | `crates/pi-ai/src/providers/models_data.rs` | Generated `flattenModelCatalog` wrapper; embedded identical JSON; differential compares the full catalog and order. |
| `ai/src/providers/google.models.ts` | 8 | 3 | done | `crates/pi-ai/src/providers/models_data.rs` | Generated `flattenModelCatalog` wrapper; embedded identical JSON; differential compares the full catalog and order. |
| `ai/src/providers/groq.models.ts` | 8 | 3 | done | `crates/pi-ai/src/providers/models_data.rs` | Generated `flattenModelCatalog` wrapper; embedded identical JSON; differential compares the full catalog and order. |
| `ai/src/providers/huggingface.models.ts` | 8 | 3 | done | `crates/pi-ai/src/providers/models_data.rs` | Generated `flattenModelCatalog` wrapper; embedded identical JSON; differential compares the full catalog and order. |
| `ai/src/providers/images/register-builtins.ts` | 50 | 3 | done | `crates/pi-ai/src/providers/images/register_builtins.rs` | Direct link replaces lazy import; load-error branch is not representable (see traceability). |
| `ai/src/providers/kimi-coding.models.ts` | 8 | 3 | done | `crates/pi-ai/src/providers/models_data.rs` | Generated `flattenModelCatalog` wrapper; embedded identical JSON; differential compares the full catalog and order. |
| `ai/src/providers/minimax-cn.models.ts` | 8 | 3 | done | `crates/pi-ai/src/providers/models_data.rs` | Generated `flattenModelCatalog` wrapper; embedded identical JSON; differential compares the full catalog and order. |
| `ai/src/providers/minimax.models.ts` | 8 | 3 | done | `crates/pi-ai/src/providers/models_data.rs` | Generated `flattenModelCatalog` wrapper; embedded identical JSON; differential compares the full catalog and order. |
| `ai/src/providers/mistral.models.ts` | 8 | 3 | done | `crates/pi-ai/src/providers/models_data.rs` | Generated `flattenModelCatalog` wrapper; embedded identical JSON; differential compares the full catalog and order. |
| `ai/src/providers/moonshotai-cn.models.ts` | 8 | 3 | done | `crates/pi-ai/src/providers/models_data.rs` | Generated `flattenModelCatalog` wrapper; embedded identical JSON; differential compares the full catalog and order. |
| `ai/src/providers/moonshotai.models.ts` | 8 | 3 | done | `crates/pi-ai/src/providers/models_data.rs` | Generated `flattenModelCatalog` wrapper; embedded identical JSON; differential compares the full catalog and order. |
| `ai/src/providers/nvidia.models.ts` | 8 | 3 | done | `crates/pi-ai/src/providers/models_data.rs` | Generated `flattenModelCatalog` wrapper; embedded identical JSON; differential compares the full catalog and order. |
| `ai/src/providers/openai-codex-gc.models.ts` | 8 | 3 | done | `crates/pi-ai/src/providers/models_data.rs` | Generated `flattenModelCatalog` wrapper; embedded identical JSON; differential compares the full catalog and order. |
| `ai/src/providers/openai-codex.models.ts` | 8 | 3 | done | `crates/pi-ai/src/providers/models_data.rs` | Generated `flattenModelCatalog` wrapper; embedded identical JSON; differential compares the full catalog and order. |
| `ai/src/providers/openai.models.ts` | 8 | 3 | done | `crates/pi-ai/src/providers/models_data.rs` | Generated `flattenModelCatalog` wrapper; embedded identical JSON; differential compares the full catalog and order. |
| `ai/src/providers/opencode-go.models.ts` | 8 | 3 | done | `crates/pi-ai/src/providers/models_data.rs` | Generated `flattenModelCatalog` wrapper; embedded identical JSON; differential compares the full catalog and order. |
| `ai/src/providers/opencode.models.ts` | 8 | 3 | done | `crates/pi-ai/src/providers/models_data.rs` | Generated `flattenModelCatalog` wrapper; embedded identical JSON; differential compares the full catalog and order. |
| `ai/src/providers/openrouter.models.ts` | 8 | 3 | done | `crates/pi-ai/src/providers/models_data.rs` | Generated `flattenModelCatalog` wrapper; embedded identical JSON; differential compares the full catalog and order. |
| `ai/src/providers/qwen-token-plan-cn.models.ts` | 8 | 3 | done | `crates/pi-ai/src/providers/models_data.rs` | Generated `flattenModelCatalog` wrapper; embedded identical JSON; differential compares the full catalog and order. |
| `ai/src/providers/qwen-token-plan-individual.models.ts` | 8 | 3 | done | `crates/pi-ai/src/providers/models_data.rs` | Generated `flattenModelCatalog` wrapper; embedded identical JSON; differential compares the full catalog and order. |
| `ai/src/providers/qwen-token-plan.models.ts` | 8 | 3 | done | `crates/pi-ai/src/providers/models_data.rs` | Generated `flattenModelCatalog` wrapper; embedded identical JSON; differential compares the full catalog and order. |
| `ai/src/providers/radius.ts` | 82 | 3 | done | `crates/pi-ai/src/providers/radius.rs` | Dynamic catalog provider with stored/legacy/network refresh; behavioral tests. |
| `ai/src/providers/together.models.ts` | 8 | 3 | done | `crates/pi-ai/src/providers/models_data.rs` | Generated `flattenModelCatalog` wrapper; embedded identical JSON; differential compares the full catalog and order. |
| `ai/src/providers/vercel-ai-gateway.models.ts` | 8 | 3 | done | `crates/pi-ai/src/providers/models_data.rs` | Generated `flattenModelCatalog` wrapper; embedded identical JSON; differential compares the full catalog and order. |
| `ai/src/providers/xai.models.ts` | 8 | 3 | done | `crates/pi-ai/src/providers/models_data.rs` | Generated `flattenModelCatalog` wrapper; embedded identical JSON; differential compares the full catalog and order. |
| `ai/src/providers/xiaomi-token-plan-ams.models.ts` | 8 | 3 | done | `crates/pi-ai/src/providers/models_data.rs` | Generated `flattenModelCatalog` wrapper; embedded identical JSON; differential compares the full catalog and order. |
| `ai/src/providers/xiaomi-token-plan-cn.models.ts` | 8 | 3 | done | `crates/pi-ai/src/providers/models_data.rs` | Generated `flattenModelCatalog` wrapper; embedded identical JSON; differential compares the full catalog and order. |
| `ai/src/providers/xiaomi-token-plan-sgp.models.ts` | 8 | 3 | done | `crates/pi-ai/src/providers/models_data.rs` | Generated `flattenModelCatalog` wrapper; embedded identical JSON; differential compares the full catalog and order. |
| `ai/src/providers/xiaomi.models.ts` | 8 | 3 | done | `crates/pi-ai/src/providers/models_data.rs` | Generated `flattenModelCatalog` wrapper; embedded identical JSON; differential compares the full catalog and order. |
| `ai/src/providers/zai-coding-cn.models.ts` | 8 | 3 | done | `crates/pi-ai/src/providers/models_data.rs` | Generated `flattenModelCatalog` wrapper; embedded identical JSON; differential compares the full catalog and order. |
| `ai/src/providers/zai.models.ts` | 8 | 3 | done | `crates/pi-ai/src/providers/models_data.rs` | Generated `flattenModelCatalog` wrapper; embedded identical JSON; differential compares the full catalog and order. |
| `ai/src/auth/oauth/github-copilot.ts` | 417 | 4 | done | `crates/pi-ai/src/auth/oauth/github_copilot.rs` | Device flow, policy enablement (concurrency 4), picker parsing, toAuth; behavioral + differential refresh/toAuth. |
| `ai/src/models.generated.ts` | 127 | 4 | done | `crates/pi-ai/src/providers/models_data.rs` | Embedded generated catalogs plus manifest timestamp; differential full catalog. |
| `ai/src/providers/amazon-bedrock.ts` | 90 | 4 | done | `crates/pi-ai/src/providers/amazon_bedrock.rs` | Factory metadata/auth/catalog; differential provider descriptors, auth resolution, logins, and request dispatch. |
| `ai/src/providers/ant-ling.ts` | 15 | 4 | done | `crates/pi-ai/src/providers/ant_ling.rs` | Factory metadata/auth/catalog; differential provider descriptors, auth resolution, logins, and request dispatch. |
| `ai/src/providers/anthropic.ts` | 59 | 4 | done | `crates/pi-ai/src/providers/anthropic.rs` | Factory metadata/auth/catalog; differential provider descriptors, auth resolution, logins, and request dispatch. |
| `ai/src/providers/azure-openai-responses.ts` | 14 | 4 | done | `crates/pi-ai/src/providers/azure_openai_responses.rs` | Factory metadata/auth/catalog; differential provider descriptors, auth resolution, logins, and request dispatch. |
| `ai/src/providers/baseten.ts` | 15 | 4 | done | `crates/pi-ai/src/providers/baseten.rs` | Factory metadata/auth/catalog; differential provider descriptors, auth resolution, logins, and request dispatch. |
| `ai/src/providers/cerebras.ts` | 15 | 4 | done | `crates/pi-ai/src/providers/cerebras.rs` | Factory metadata/auth/catalog; differential provider descriptors, auth resolution, logins, and request dispatch. |
| `ai/src/providers/cloudflare-ai-gateway.ts` | 23 | 4 | done | `crates/pi-ai/src/providers/cloudflare_ai_gateway.rs` | Factory metadata/auth/catalog; differential provider descriptors, auth resolution, logins, and request dispatch. |
| `ai/src/providers/cloudflare-workers-ai.ts` | 15 | 4 | done | `crates/pi-ai/src/providers/cloudflare_workers_ai.rs` | Factory metadata/auth/catalog; differential provider descriptors, auth resolution, logins, and request dispatch. |
| `ai/src/providers/deepseek.ts` | 15 | 4 | done | `crates/pi-ai/src/providers/deepseek.rs` | Factory metadata/auth/catalog; differential provider descriptors, auth resolution, logins, and request dispatch. |
| `ai/src/providers/fireworks.ts` | 19 | 4 | done | `crates/pi-ai/src/providers/fireworks.rs` | Factory metadata/auth/catalog; differential provider descriptors, auth resolution, logins, and request dispatch. |
| `ai/src/providers/github-copilot.ts` | 34 | 4 | done | `crates/pi-ai/src/providers/github_copilot.rs` | Factory metadata/auth/catalog; differential provider descriptors, auth resolution, logins, and request dispatch. |
| `ai/src/providers/google-vertex.ts` | 100 | 4 | done | `crates/pi-ai/src/providers/google_vertex.rs` | Factory metadata/auth/catalog; differential provider descriptors, auth resolution, logins, and request dispatch. |
| `ai/src/providers/google.ts` | 15 | 4 | done | `crates/pi-ai/src/providers/google.rs` | Factory metadata/auth/catalog; differential provider descriptors, auth resolution, logins, and request dispatch. |
| `ai/src/providers/groq.ts` | 15 | 4 | done | `crates/pi-ai/src/providers/groq.rs` | Factory metadata/auth/catalog; differential provider descriptors, auth resolution, logins, and request dispatch. |
| `ai/src/providers/huggingface.ts` | 15 | 4 | done | `crates/pi-ai/src/providers/huggingface.rs` | Factory metadata/auth/catalog; differential provider descriptors, auth resolution, logins, and request dispatch. |
| `ai/src/providers/kimi-coding.ts` | 24 | 4 | done | `crates/pi-ai/src/providers/kimi_coding.rs` | Factory metadata/auth/catalog; differential provider descriptors, auth resolution, logins, and request dispatch. |
| `ai/src/providers/minimax-cn.ts` | 15 | 4 | done | `crates/pi-ai/src/providers/minimax_cn.rs` | Factory metadata/auth/catalog; differential provider descriptors, auth resolution, logins, and request dispatch. |
| `ai/src/providers/minimax.ts` | 15 | 4 | done | `crates/pi-ai/src/providers/minimax.rs` | Factory metadata/auth/catalog; differential provider descriptors, auth resolution, logins, and request dispatch. |
| `ai/src/providers/mistral.ts` | 15 | 4 | done | `crates/pi-ai/src/providers/mistral.rs` | Factory metadata/auth/catalog; differential provider descriptors, auth resolution, logins, and request dispatch. |
| `ai/src/providers/moonshotai-cn.ts` | 15 | 4 | done | `crates/pi-ai/src/providers/moonshotai_cn.rs` | Factory metadata/auth/catalog; differential provider descriptors, auth resolution, logins, and request dispatch. |
| `ai/src/providers/moonshotai.ts` | 15 | 4 | done | `crates/pi-ai/src/providers/moonshotai.rs` | Factory metadata/auth/catalog; differential provider descriptors, auth resolution, logins, and request dispatch. |
| `ai/src/providers/nvidia.ts` | 15 | 4 | done | `crates/pi-ai/src/providers/nvidia.rs` | Factory metadata/auth/catalog; differential provider descriptors, auth resolution, logins, and request dispatch. |
| `ai/src/providers/openai-codex.ts` | 25 | 4 | todo |  | Excluded from this slice: depends on the in-flight api-codex adapter. |
| `ai/src/providers/openai.ts` | 15 | 4 | done | `crates/pi-ai/src/providers/openai.rs` | Factory metadata/auth/catalog; differential provider descriptors, auth resolution, logins, and request dispatch. |
| `ai/src/providers/opencode-go.ts` | 20 | 4 | done | `crates/pi-ai/src/providers/opencode_go.rs` | Factory metadata/auth/catalog; differential provider descriptors, auth resolution, logins, and request dispatch. |
| `ai/src/providers/opencode.ts` | 24 | 4 | done | `crates/pi-ai/src/providers/opencode.rs` | Factory metadata/auth/catalog; differential provider descriptors, auth resolution, logins, and request dispatch. |
| `ai/src/providers/openrouter-images.ts` | 22 | 4 | done | `crates/pi-ai/src/providers/openrouter_images.rs` | Catalog, api-key auth, lazy OAuth surface; OAuth flow module remains an explicit gap (see traceability). |
| `ai/src/providers/openrouter.ts` | 23 | 4 | done | `crates/pi-ai/src/providers/openrouter.rs` | Factory metadata/auth/catalog; differential provider descriptors, auth resolution, logins, and request dispatch. |
| `ai/src/providers/qwen-token-plan-cn.ts` | 15 | 4 | done | `crates/pi-ai/src/providers/qwen_token_plan_cn.rs` | Factory metadata/auth/catalog; differential provider descriptors, auth resolution, logins, and request dispatch. |
| `ai/src/providers/qwen-token-plan-individual.ts` | 15 | 4 | done | `crates/pi-ai/src/providers/qwen_token_plan_individual.rs` | Factory metadata/auth/catalog; differential provider descriptors, auth resolution, logins, and request dispatch. |
| `ai/src/providers/qwen-token-plan.ts` | 15 | 4 | done | `crates/pi-ai/src/providers/qwen_token_plan.rs` | Factory metadata/auth/catalog; differential provider descriptors, auth resolution, logins, and request dispatch. |
| `ai/src/providers/together.ts` | 15 | 4 | done | `crates/pi-ai/src/providers/together.rs` | Factory metadata/auth/catalog; differential provider descriptors, auth resolution, logins, and request dispatch. |
| `ai/src/providers/vercel-ai-gateway.ts` | 15 | 4 | done | `crates/pi-ai/src/providers/vercel_ai_gateway.rs` | Factory metadata/auth/catalog; differential provider descriptors, auth resolution, logins, and request dispatch. |
| `ai/src/providers/xai.ts` | 28 | 4 | done | `crates/pi-ai/src/providers/xai.rs` | Factory metadata/auth/catalog; differential provider descriptors, auth resolution, logins, and request dispatch. |
| `ai/src/providers/xiaomi-token-plan-ams.ts` | 15 | 4 | done | `crates/pi-ai/src/providers/xiaomi_token_plan_ams.rs` | Factory metadata/auth/catalog; differential provider descriptors, auth resolution, logins, and request dispatch. |
| `ai/src/providers/xiaomi-token-plan-cn.ts` | 15 | 4 | done | `crates/pi-ai/src/providers/xiaomi_token_plan_cn.rs` | Factory metadata/auth/catalog; differential provider descriptors, auth resolution, logins, and request dispatch. |
| `ai/src/providers/xiaomi-token-plan-sgp.ts` | 15 | 4 | done | `crates/pi-ai/src/providers/xiaomi_token_plan_sgp.rs` | Factory metadata/auth/catalog; differential provider descriptors, auth resolution, logins, and request dispatch. |
| `ai/src/providers/xiaomi.ts` | 15 | 4 | done | `crates/pi-ai/src/providers/xiaomi.rs` | Factory metadata/auth/catalog; differential provider descriptors, auth resolution, logins, and request dispatch. |
| `ai/src/providers/zai-coding-cn.ts` | 15 | 4 | done | `crates/pi-ai/src/providers/zai_coding_cn.rs` | Factory metadata/auth/catalog; differential provider descriptors, auth resolution, logins, and request dispatch. |
| `ai/src/providers/zai.ts` | 15 | 4 | done | `crates/pi-ai/src/providers/zai.rs` | Factory metadata/auth/catalog; differential provider descriptors, auth resolution, logins, and request dispatch. |
| `ai/src/bun-oauth.ts` | 21 | 5 | done | `crates/pi-ai/src/bun_oauth.rs` | Registers all ported flows, including GitHub Copilot. |
| `ai/src/providers/all.ts` | 159 | 5 | partial | `crates/pi-ai/src/providers/all.rs` | All factories except the two `openai-codex` providers (api-codex slice); catalog/order/generatedAt/images differential. |
| `ai/src/cli.ts` | 119 | 6 | todo |  |  |
| `ai/src/compat.ts` | 299 | 6 | todo |  |  |
| `ai/src/base.ts` | 6 | 7 | todo |  |  |

## agent (47 files, 12044 lines)

| Source | Lines | Level | Status | Rust | Notes |
|---|---:|---:|---|---|---|
| `agent/src/harness/events.ts` | 102 | 0 | todo |  |  |
| `agent/src/harness/result.ts` | 63 | 0 | todo |  |  |
| `agent/src/harness/tools/edit-diff.ts` | 500 | 0 | done | `crates/pi-agent-core/src/harness/tools/edit_diff.rs` | Fuzzy matching, replacements, jsdiff line diff/unified patch; 8 behavioral + edit differential fixtures; see `edit-diff.md`. |
| `agent/src/harness/tools/image.ts` | 104 | 0 | done | `crates/pi-agent-core/src/harness/tools/image.rs` | MIME detection + Base64; focused and differential proof; see `image-helpers.md`. |
| `agent/src/harness/utils/truncate.ts` | 350 | 0 | done | `crates/pi-agent-core/src/harness/utils/truncate.rs` | Head/tail/line truncation and size formatting; focused and 34-request differential proof; see `truncation.md`. |
| `agent/src/harness/telemetry.ts` | 615 | 1 | todo |  |  |
| `agent/src/proxy.ts` | 370 | 4 | done | `crates/pi-agent-core/src/agent/proxy.rs` | Proxy SSE reconstruction, error/abort paths, injectable `Fetch`; see `agent-proxy.md`. |
| `agent/src/types.ts` | 470 | 4 | done | `crates/pi-agent-core/src/agent/types.rs` | Full export set; `AgentMessage` custom variant is a raw JSON object; see `agent-types.md`. |
| `agent/src/harness/compaction/utils.ts` | 163 | 5 | partial |  |  |
| `agent/src/harness/messages.ts` | 188 | 5 | partial |  |  |
| `agent/src/harness/session/session.ts` | 340 | 5 | done | `crates/pi-agent-core/src/harness/session/facade.rs` | Session facade, lane views, query/limit validation, payload checks; see `session-facade.md`. |
| `agent/src/harness/session/types.ts` | 406 | 5 | done | `crates/pi-agent-core/src/harness/session/types.rs` | Typed entries/records/queries/errors; `OptionalValue` keeps explicit null; see `session-types.md`. |
| `agent/src/harness/types.ts` | 320 | 5 | partial | `crates/pi-agent-core/src/harness/types.rs` | FileSystem/Shell/ExecutionEnv, options, errors and records ported; Result helpers are Rust idioms; harness tool/stream-option types await harness orchestration; see `harness-types.md`. |
| `agent/src/stream-fn.ts` | 20 | 5 | done | `crates/pi-agent-core/src/agent/stream_fn.rs` | Process-global default stream fn; exact missing-default error; see `agent-stream-fn.md`. |
| `agent/src/agent-loop.ts` | 1028 | 6 | done | `crates/pi-agent-core/src/agent/agent_loop.rs` | Event ordering, tool modes, steering/follow-up, termination, abort races, restored tool calls, listener failures, result capping; 37 behavioral tests + 34 differential scenarios; see `agent-loop.md`. |
| `agent/src/harness/env/nodejs.ts` | 695 | 6 | done | `crates/pi-agent-core/src/harness/env/nodejs.rs` | tokio fs/process env, shell selection, env handling, timeout/abort/process-group kill, streaming; 23 tests; Windows branches excluded; see `harness-nodejs-env.md`. |
| `agent/src/harness/prompt-templates.ts` | 262 | 6 | partial |  |  |
| `agent/src/harness/reducer.ts` | 667 | 6 | todo |  |  |
| `agent/src/harness/session/context.ts` | 100 | 6 | done | `crates/pi-agent-core/src/harness/session/context.rs` | Compaction boundary, transforms, projectors, model/thinking derivation; see `session-context.md`. |
| `agent/src/harness/session/jsonl/errors.ts` | 27 | 6 | done | `crates/pi-agent-core/src/harness/session/jsonl/errors.rs` | File/decode/replay cause mapping; see `session-jsonl-errors.md`. |
| `agent/src/harness/session/jsonl/types.ts` | 57 | 6 | done | `crates/pi-agent-core/src/harness/session/jsonl/types.rs` | FS capability subset + native adapter; see `session-jsonl-types.md`. |
| `agent/src/harness/session/state.ts` | 344 | 6 | done | `crates/pi-agent-core/src/harness/session/state.rs` | Mutations, branch walks, queries, fork mutations, stats; see `session-state.md`. |
| `agent/src/harness/skills.ts` | 375 | 6 | todo |  |  |
| `agent/src/harness/system-prompt.ts` | 34 | 6 | todo |  |  |
| `agent/src/harness/tools/file-mutation-queue.ts` | 56 | 6 | done | `crates/pi-agent-core/src/harness/tools/file_mutation_queue.rs` | Canonical-path serialization shared via `ExecutionToolContext`; see `file-mutation-queue.md`. |
| `agent/src/harness/tools/path-utils.ts` | 30 | 6 | done | `crates/pi-agent-core/src/harness/tools/path_utils.rs` | Path normalization and read variants; see `path-utils.md`. |
| `agent/src/harness/tools/tool-context.ts` | 6 | 6 | done | `crates/pi-agent-core/src/harness/tools/tool_context.rs` | Env + mutation queue bound at tool construction; see `tool-context.md`. |
| `agent/src/harness/utils/shell-output.ts` | 199 | 6 | done | `crates/pi-agent-core/src/harness/utils/shell_output.rs` | Sanitizer + full `executeShellWithCapture`; 6 tests and bash differential fixtures; see `shell-output.md`. |
| `agent/src/search/index.ts` | 32 | 6 | done | `crates/pi-agent-core/src/search/index.rs` | Search options/hit traits; see `search-index.md`. |
| `agent/src/search/scanning.ts` | 176 | 6 | done | `crates/pi-agent-core/src/search/scanning.rs` | Paged scanning, labels, filters, abort, custom hits; see `search-scanning.md`. |
| `agent/src/agent.ts` | 626 | 7 | done | `crates/pi-agent-core/src/agent/agent.rs` | Stateful wrapper, queues, lifecycle, listener settlement and failure propagation, continue/reset; 23 tests + 6 differential scenarios; see `agent.md`. |
| `agent/src/harness/compaction/compaction.ts` | 910 | 7 | partial |  |  |
| `agent/src/harness/session/jsonl/codec.ts` | 240 | 7 | done | `crates/pi-agent-core/src/harness/session/jsonl/codec.rs` | Byte-identical header/mutation codec with TS key order and js_number; see `session-jsonl-codec.md`. |
| `agent/src/harness/session/memory.ts` | 192 | 7 | done | `crates/pi-agent-core/src/harness/session/memory.rs` | In-memory storage/repo with injectable clock and ids; see `session-memory.md`. |
| `agent/src/harness/tools/bash.ts` | 161 | 7 | done | `crates/pi-agent-core/src/harness/tools/bash.rs` | Schema, prepare/prefix, throttled updates, truncation/error/abort mapping; 8 behavioral + 7 differential fixtures; see `harness-tools-bash.md`. |
| `agent/src/harness/tools/edit.ts` | 127 | 7 | done | `crates/pi-agent-core/src/harness/tools/edit.rs` | Legacy argument preparation, queued edits, diff/patch details; 8 behavioral + 11 differential fixtures; see `harness-tools-edit.md`. |
| `agent/src/harness/tools/read.ts` | 144 | 7 | done | `crates/pi-agent-core/src/harness/tools/read.rs` | Text offsets/limits/truncation, image detection and processor; 5 behavioral + 9 differential fixtures; see `harness-tools-read.md`. |
| `agent/src/harness/tools/write.ts` | 39 | 7 | done | `crates/pi-agent-core/src/harness/tools/write.rs` | Queued writes, abort checks, UTF-16 byte count; 2 tests + 3 differential fixtures; see `harness-tools-write.md`. |
| `agent/src/harness/session/jsonl/storage.ts` | 277 | 8 | done | `crates/pi-agent-core/src/harness/session/jsonl/storage.rs` | Serialized writes, torn-tail repair, atomic publish/fork; see `session-jsonl-storage.md`. |
| `agent/src/harness/tools/index.ts` | 23 | 8 | done | `crates/pi-agent-core/src/harness/tools/mod.rs` | Tool module re-exports; input/details are JSON values; see `harness-tools-index.md`. |
| `agent/src/harness/session/jsonl/repo.ts` | 247 | 9 | done | `crates/pi-agent-core/src/harness/session/jsonl/repo.rs` | Directory/file naming, id validation, listing, create/fork claims; see `session-jsonl-repo.md`. |
| `agent/src/harness/session/jsonl.ts` | 9 | 10 | done | `crates/pi-agent-core/src/harness/session/jsonl/mod.rs` | Barrel re-exports; see `session-index.md`. |
| `agent/src/harness/session/index.ts` | 13 | 11 | done | `crates/pi-agent-core/src/harness/session/mod.rs` | Session package exports; see `session-index.md`. |
| `agent/src/harness/agent-harness.ts` | 508 | 12 | todo |  |  |
| `agent/src/harness/compaction/branch-summarization.ts` | 282 | 12 | todo |  |  |
| `agent/src/index.ts` | 145 | 13 | partial |  |  |
| `agent/src/node.ts` | 2 | 14 | todo |  |  |

## tui (41 files, 18529 lines)

| Source | Lines | Level | Status | Rust | Notes |
|---|---:|---:|---|---|---|
| `tui/src/fuzzy.ts` | 161 | 0 | done | `crates/pi-tui/src/fuzzy.rs` | exact score-bit differential |
| `tui/src/keys.ts` | 1404 | 0 | done | `crates/pi-tui/src/keys.rs` | differential legacy/Kitty/modifyOther matrix; Key helper gap |
| `tui/src/kill-ring.ts` | 46 | 0 | done | `crates/pi-tui/src/kill_ring.rs` | differential ring operations |
| `tui/src/native-modifiers.ts` | 66 | 0 | partial | `crates/pi-tui/src/native_modifiers.rs` | constant false; Node N-API addon path unported (gap) |
| `tui/src/stdin-buffer.ts` | 444 | 0 | done | `crates/pi-tui/src/stdin_buffer.rs` | differential chunk/paste matrix; driver-owned timer API |
| `tui/src/terminal-colors.ts` | 73 | 0 | done | `crates/pi-tui/src/terminal_colors.rs` | differential parse matrix; TUI query method out of leaf scope |
| `tui/src/terminal-image.ts` | 657 | 0 | done | `crates/pi-tui/src/terminal_image.rs` + internal `base64.rs` | differential encode/detect/render/placement/dimensions; WebP <30 source bug preserved |
| `tui/src/undo-stack.ts` | 28 | 0 | done | `crates/pi-tui/src/undo_stack.rs` | clone-on-push; differential N/A (structuredClone) |
| `tui/src/utils.ts` | 1326 | 0 | done | `crates/pi-tui/src/utils.rs` + generated `unicode_tables.rs` | Unicode 17.0 tables; differential corpus + ~11k codepoint samples |
| `tui/src/autocomplete.ts` | 793 | 1 | done | `crates/pi-tui/src/autocomplete.rs` | TS test port + real-fd differential; async argument-completion callback and locale collation documented as gaps |
| `tui/src/keybindings.ts` | 325 | 1 | done | `crates/pi-tui/src/keybindings.rs` | differential resolved keys/conflicts; ordered config pairs |
| `tui/src/latex.ts` | 1373 | 1 | done | `crates/pi-tui/src/latex.rs` | TS test suite + generated differential corpus; byte-identical output incl. layout markers |
| `tui/src/terminal.ts` | 594 | 1 | done | `crates/pi-tui/src/terminal.rs` | differential negotiation/timeout/normalization; ProcessTerminal event-queue seam; resize polling |
| `tui/src/word-navigation.ts` | 117 | 1 | done | `crates/pi-tui/src/word_navigation.rs` | TS tests + boundary differential; ICU4X dictionary-word mismatches pinned as approved deviation |
| `tui/src/tui.ts` | 2534 | 2 | done | `crates/pi-tui/src/tui.rs` | differential byte stream (main+renderer), overlays/focus/fixed cells/render regions, viewport helpers and start/stop hooks |
| `tui/src/components/alt-screen-flash.ts` | 51 | 3 | done | `crates/pi-tui/src/components/alt_screen_flash.rs` | differential flash/render/dispose; expiry timer driver-owned |
| `tui/src/components/box.ts` | 137 | 3 | done | `crates/pi-tui/src/components/box.rs` | differential padding/background/cache-sample matrix |
| `tui/src/components/dynamic-border.ts` | 15 | 3 | done | `crates/pi-tui/src/components/dynamic_border.rs` | TS tests ported + differential |
| `tui/src/components/image.ts` | 127 | 3 | done | `crates/pi-tui/src/components/image.rs` | differential kitty/iterm2/fallback renders; kitty ids explicit |
| `tui/src/components/input.ts` | 465 | 3 | done | `crates/pi-tui/src/components/input.rs` | TS tests ported + scripted differential edits/renders |
| `tui/src/components/markdown.ts` | 1027 | 3 | done | `crates/pi-tui/src/components/markdown/` | marked 18.0.5 lexer port (fancy-regex) + component; token-stream and ANSI render differential; all TS tests ported |
| `tui/src/components/select-list.ts` | 229 | 3 | done | `crates/pi-tui/src/components/select_list.rs` | TS tests ported; differential render/callback matrix |
| `tui/src/components/spacer.ts` | 28 | 3 | done | `crates/pi-tui/src/components/spacer.rs` | differential line counts + setLines |
| `tui/src/components/text.ts` | 106 | 3 | done | `crates/pi-tui/src/components/text.rs` | TS differential wrapping/padding/background/cache |
| `tui/src/components/truncated-text.ts` | 65 | 3 | done | `crates/pi-tui/src/components/truncated_text.rs` | all TS tests ported + differential |
| `tui/src/editor-component.ts` | 74 | 3 | done | `crates/pi-tui/src/editor_component.rs` | Optional members mapped to default trait methods; typed callback fields ([traceability](traceability/editor-component.md)). |
| `tui/src/layout-node.ts` | 51 | 3 | done | `crates/pi-tui/src/layout_node.rs` | `Component::layout_node` replaces the `LAYOUT_NODE` symbol |
| `tui/src/root-compositor.ts` | 91 | 3 | done | `crates/pi-tui/src/root_compositor.rs` | flow/bottom layout callbacks covered by render-region tests |
| `tui/src/tui-main-screen.ts` | 586 | 3 | done | `crates/pi-tui/src/tui_main_screen.rs` | differential byte stream; capture/restore implemented (no listed test) |
| `tui/src/alt-screen-search.ts` | 157 | 4 | done | `crates/pi-tui/src/alt_screen_search.rs` | TS search cases ported; differential match/segment/key and component render/query scripts |
| `tui/src/components/loader.ts` | 94 | 4 | done | `crates/pi-tui/src/components/loader.rs` | TS test port + differential; frame timer is driver-owned (`poll`) |
| `tui/src/components/scroll-view.ts` | 216 | 4 | done | `crates/pi-tui/src/components/scroll_view.rs` | TS layout tests ported; differential geometry/scroll state; scrollbar timer driver-owned |
| `tui/src/components/settings-list.ts` | 249 | 4 | done | `crates/pi-tui/src/components/settings_list.rs` | TS tests ported + differential search/cycle/submenu/scroll matrix |
| `tui/src/components/stack.ts` | 154 | 4 | done | `crates/pi-tui/src/components/stack.rs` | size allocation covered by layout tests + differential |
| `tui/src/components/cancellable-loader.ts` | 40 | 5 | done | `crates/pi-tui/src/components/cancellable_loader.rs` | differential escape abort + render; AbortSignal mapped to CancellationToken |
| `tui/src/components/editor.ts` | 2687 | 5 | done | `crates/pi-tui/src/components/editor.rs` | Full TS test port (202 cases) + 28-scenario differential against real TS editor; timer/async requests driver-owned ([traceability](traceability/editor.md)). |
| `tui/src/components/h-stack.ts` | 44 | 5 | done | `crates/pi-tui/src/components/h_stack.rs` | TS layout tests + differential composition/alignment |
| `tui/src/components/v-stack.ts` | 33 | 5 | done | `crates/pi-tui/src/components/v_stack.rs` | TS layout tests + differential sizes/gaps |
| `tui/src/layout.ts` | 410 | 5 | done | `crates/pi-tui/src/layout.rs` | TS layout tests ported; differential frame geometry, scrollbar, kitty crop |
| `tui/src/tui-alt-screen.ts` | 1291 | 6 | done | `crates/pi-tui/src/tui_alt_screen.rs` | 36 TS test cases + 23-scenario differential (full write stream and viewport/scroll/search state) |
| `tui/src/index.ts` | 161 | 7 | partial | `crates/pi-tui/src/lib.rs` | Package-root re-exports; Editor/EditorComponent, Marked/Token/Tokens, Key and provider/interface aliases pending (see tui-index traceability) |

## client (10 files, 1225 lines)

| Source | Lines | Level | Status | Rust | Notes |
|---|---:|---:|---|---|---|
| `client/src/promise.ts` | 16 | 0 | done | `crates/pi-client/src/promise.rs` | One-shot promise resolver over `tokio::sync::oneshot` |
| `client/src/transport.ts` | 18 | 0 | done | `crates/pi-client/src/transport.rs` | Eager `send` method returning completion future; trait factory |
| `client/src/errors.ts` | 56 | 5 | done | `crates/pi-client/src/errors.rs` | Typed error enum; `PiError::message` gap documented |
| `client/src/types.ts` | 26 | 5 | done | `crates/pi-client/src/types.rs` | Enums and listener aliases |
| `client/src/unix.ts` | 156 | 5 | done | `crates/pi-client/src/unix.rs` | Tokio Unix socket transport; real-socket tests |
| `client/src/connection.ts` | 236 | 6 | done | `crates/pi-client/src/connection.rs` | Full lifecycle, reentrant callbacks; connection test port |
| `client/src/session-handle.ts` | 111 | 6 | done | `crates/pi-client/src/session_handle.rs` | Lease callbacks trait; command futures |
| `client/src/state.ts` | 156 | 6 | done | `crates/pi-client/src/state.rs` | Snapshot reduction, listener isolation |
| `client/src/client.ts` | 432 | 7 | done | `crates/pi-client/src/client.rs` | Requests, leases, attachment serialization |
| `client/src/index.ts` | 18 | 8 | done | `crates/pi-client/src/lib.rs` | Public re-exports; static connect renamed `connect_new` |

## server (17 files, 2305 lines)

| Source | Lines | Level | Status | Rust | Notes |
|---|---:|---:|---|---|---|
| `server/src/errors.ts` | 58 | 5 | done | `crates/pi-server/src/errors.rs` | Typed operation codes, cause-retaining internal error |
| `server/src/protocol.ts` | 388 | 5 | done | `crates/pi-server/src/protocol.rs` | pi-ai → protocol bridge; differential oracle |
| `server/src/connection.ts` | 36 | 6 | done | `crates/pi-server/src/connection.rs` | Byte connection/handler contracts, stage state |
| `server/src/listener.ts` | 10 | 6 | done | `crates/pi-server/src/listener.rs` | Listener trait |
| `server/src/types.ts` | 63 | 6 | done | `crates/pi-server/src/types.rs` | Service/runtime traits and option records |
| `server/src/sessions.ts` | 346 | 7 | done | `crates/pi-server/src/sessions.rs` | Live session manager, disposal, metadata merge |
| `server/src/snapshots.ts` | 62 | 7 | done | `crates/pi-server/src/snapshots.rs` | Serialized revision publisher |
| `server/src/transports/unix/types.ts` | 15 | 7 | done | `crates/pi-server/src/transports/unix/types.rs` | Listener/server options |
| `server/src/server.ts` | 396 | 8 | done | `crates/pi-server/src/server.rs` | Handshake, dispatch, close, error mapping |
| `server/src/transports/unix/listener.ts` | 434 | 8 | done | `crates/pi-server/src/transports/unix/listener.rs` | Stale sockets, hard-link bind, pending-byte writes |
| `server/src/index.ts` | 5 | 9 | done | `crates/pi-server/src/lib.rs` | Re-exports |
| `server/src/transports/unix/preset.ts` | 23 | 9 | done | `crates/pi-server/src/transports/unix/preset.rs` | Unix server composition |
| `server/src/transports/unix/index.ts` | 3 | 10 | done | `crates/pi-server/src/transports/unix/mod.rs` | Transport re-exports |
| `server/src/testing/client.ts` | 147 | 6 | done | `crates/pi-server/src/testing/client.rs` | Wire client used by all integration tests |
| `server/src/testing/index.ts` | 5 | 6 | done | `crates/pi-server/src/testing/mod.rs` | Testing re-exports |
| `server/src/testing/server.ts` | 27 | 6 | done | `crates/pi-server/src/testing/server.rs` | Deterministic test server factory |
| `server/src/testing/service.ts` | 287 | 6 | done | `crates/pi-server/src/testing/service.rs` | In-memory service/runtime with shared snapshots |

## session-backends (18 files, 2389 lines)

| Source | Lines | Level | Status | Rust | Notes |
|---|---:|---:|---|---|---|
| `session-backends/sqlite-node/src/sqlite/types.ts` | 52 | 14 | todo |  |  |
| `session-backends/sqlite-node/src/sqlite/sql.ts` | 66 | 15 | todo |  |  |
| `session-backends/sqlite-node/src/sqlite/migrations.ts` | 49 | 16 | todo |  |  |
| `session-backends/sqlite-node/src/sqlite/storage/branch-entries.ts` | 174 | 16 | todo |  |  |
| `session-backends/sqlite-node/src/sqlite/storage/branch-tips.ts` | 35 | 16 | todo |  |  |
| `session-backends/sqlite-node/src/sqlite/storage/entries.ts` | 78 | 16 | todo |  |  |
| `session-backends/sqlite-node/src/sqlite/storage/facts.ts` | 64 | 16 | todo |  |  |
| `session-backends/sqlite-node/src/sqlite/storage/lanes.ts` | 124 | 16 | todo |  |  |
| `session-backends/sqlite-node/src/sqlite/storage/records.ts` | 95 | 16 | todo |  |  |
| `session-backends/sqlite-node/src/sqlite/storage/session-sequences.ts` | 29 | 16 | todo |  |  |
| `session-backends/sqlite-node/src/sqlite/storage/session-stats.ts` | 54 | 16 | todo |  |  |
| `session-backends/sqlite-node/src/sqlite/storage/sessions.ts` | 131 | 16 | todo |  |  |
| `session-backends/sqlite-node/src/sqlite/storage/writer-leases.ts` | 58 | 16 | todo |  |  |
| `session-backends/sqlite-node/src/sqlite/branch-cache.ts` | 101 | 17 | todo |  |  |
| `session-backends/sqlite-node/src/sqlite/search-backend.ts` | 194 | 17 | todo |  |  |
| `session-backends/sqlite-node/src/sqlite/repo.ts` | 953 | 18 | todo |  |  |
| `session-backends/sqlite-node/src/sqlite/index.ts` | 18 | 19 | todo |  |  |
| `session-backends/sqlite-node/src/index.ts` | 114 | 20 | todo |  |  |

## coding-agent (346 files, 101231 lines)

| Source | Lines | Level | Status | Rust | Notes |
|---|---:|---:|---|---|---|
| `coding-agent/extensions/goal/src/goal-args.ts` | 34 | 0 | todo |  |  |
| `coding-agent/extensions/goal/src/goal-objective.ts` | 14 | 0 | todo |  |  |
| `coding-agent/extensions/goal/src/stale-context.ts` | 5 | 0 | todo |  |  |
| `coding-agent/extensions/pyrun/src/runner.ts` | 311 | 0 | todo |  |  |
| `coding-agent/src/bun/restore-sandbox-env.ts` | 36 | 0 | todo |  |  |
| `coding-agent/src/cli/experimental/auth.ts` | 21 | 0 | todo |  |  |
| `coding-agent/src/cli/experimental/command.ts` | 205 | 0 | todo |  |  |
| `coding-agent/src/cli/experimental/transport-address.ts` | 48 | 0 | todo |  |  |
| `coding-agent/src/core/debug-repl.ts` | 292 | 0 | todo |  |  |
| `coding-agent/src/core/desktop-notification.ts` | 116 | 0 | todo |  |  |
| `coding-agent/src/core/detached-job-retention.ts` | 76 | 0 | todo |  |  |
| `coding-agent/src/core/diagnostics.ts` | 15 | 0 | todo |  |  |
| `coding-agent/src/core/event-bus.ts` | 33 | 0 | todo |  |  |
| `coding-agent/src/core/experimental.ts` | 9 | 0 | todo |  |  |
| `coding-agent/src/core/export-html/ansi-to-html.ts` | 258 | 0 | todo |  |  |
| `coding-agent/src/core/http-dispatcher.ts` | 111 | 0 | todo |  |  |
| `coding-agent/src/core/output-guard.ts` | 126 | 0 | todo |  |  |
| `coding-agent/src/core/permissions/policy.ts` | 33 | 0 | todo |  |  |
| `coding-agent/src/core/pi-manifest.ts` | 34 | 0 | todo |  |  |
| `coding-agent/src/core/provider-display-names.ts` | 34 | 0 | todo |  |  |
| `coding-agent/src/core/radius.ts` | 1 | 0 | todo |  |  |
| `coding-agent/src/core/resident-console-transport.ts` | 414 | 0 | todo |  |  |
| `coding-agent/src/core/runtime-process.ts` | 103 | 0 | todo |  |  |
| `coding-agent/src/core/session-errors.ts` | 12 | 0 | todo |  |  |
| `coding-agent/src/core/session-health.ts` | 135 | 0 | todo |  |  |
| `coding-agent/src/core/skill-block.ts` | 22 | 0 | partial |  |  |
| `coding-agent/src/core/sqlite.ts` | 145 | 0 | todo |  |  |
| `coding-agent/src/core/timings.ts` | 50 | 0 | todo |  |  |
| `coding-agent/src/core/tool-capabilities.ts` | 11 | 0 | todo |  |  |
| `coding-agent/src/core/tool-detach-registry.ts` | 58 | 0 | todo |  |  |
| `coding-agent/src/core/tools/file-mutation-queue.ts` | 61 | 0 | todo |  |  |
| `coding-agent/src/core/tools/truncate.ts` | 276 | 0 | partial |  |  |
| `coding-agent/src/extensions/llama/client.ts` | 332 | 0 | todo |  |  |
| `coding-agent/src/extensions/llama/huggingface.ts` | 158 | 0 | todo |  |  |
| `coding-agent/src/modes/interactive/components/elapsed-time.ts` | 30 | 0 | todo |  |  |
| `coding-agent/src/modes/interactive/external-editor.ts` | 45 | 0 | todo |  |  |
| `coding-agent/src/modes/interactive/model-search.ts` | 27 | 0 | todo |  |  |
| `coding-agent/src/modes/rpc/jsonl.ts` | 58 | 0 | partial |  |  |
| `coding-agent/src/supervisor/project-resolver.ts` | 108 | 0 | todo |  |  |
| `coding-agent/src/supervisor/request-wake.ts` | 149 | 0 | todo |  |  |
| `coding-agent/src/utils/abort.ts` | 48 | 0 | done | `crates/pi-coding-agent/src/utils/abort.rs` | Focused tests; no custom abort reasons |
| `coding-agent/src/utils/ansi.ts` | 60 | 0 | done | `crates/pi-coding-agent/src/utils/ansi.rs` | Differential 26001 (237 inputs); focused tests |
| `coding-agent/src/utils/child-process.ts` | 137 | 0 | done | `crates/pi-coding-agent/src/utils/child_process.rs` | Focused process tests; collects stdout/stderr |
| `coding-agent/src/utils/clipboard-native.ts` | 33 | 0 | partial | `crates/pi-coding-agent/src/utils/clipboard_native.rs` | Loader/display ported; native npm binding gap |
| `coding-agent/src/utils/clipboard-temp-files.ts` | 42 | 0 | done | `crates/pi-coding-agent/src/utils/clipboard_temp_files.rs` | Focused tracker tests |
| `coding-agent/src/utils/deprecation.ts` | 14 | 0 | done | `crates/pi-coding-agent/src/utils/deprecation.rs` | Focused dedupe/yellow tests; chalk dependency port `utils::chalk` (see `traceability/chalk.md`) |
| `coding-agent/src/utils/footer-status.ts` | 20 | 0 | done | `crates/pi-coding-agent/src/utils/footer_status.rs` | Differential 26004; focused tests |
| `coding-agent/src/utils/frontmatter.ts` | 39 | 0 | done | `crates/pi-coding-agent/src/utils/frontmatter.rs` | Differential 26007; focused tests |
| `coding-agent/src/utils/fs-watch.ts` | 30 | 0 | done | `crates/pi-coding-agent/src/utils/fs_watch.rs` | Focused notify-backed tests |
| `coding-agent/src/utils/git-worktree.ts` | 113 | 0 | done | `crates/pi-coding-agent/src/utils/git_worktree.rs` | Differential 26011; real git tests |
| `coding-agent/src/utils/git.ts` | 226 | 0 | done | `crates/pi-coding-agent/src/utils/git.rs` | Differential 26008 (55 cases); focused tests |
| `coding-agent/src/utils/html.ts` | 51 | 0 | done | `crates/pi-coding-agent/src/utils/html.rs` | Differential 26002; focused tests |
| `coding-agent/src/utils/json.ts` | 6 | 0 | done | `crates/pi-coding-agent/src/utils/json.rs` | Differential 26003; focused tests |
| `coding-agent/src/utils/management-http.ts` | 68 | 0 | done | `crates/pi-coding-agent/src/utils/management_http.rs` | Mock retry matrix plus real local server |
| `coding-agent/src/utils/mime.ts` | 116 | 0 | done | `crates/pi-coding-agent/src/utils/mime.rs` | Differential 26010 (33 buffers); focused tests |
| `coding-agent/src/utils/open-browser.ts` | 24 | 0 | done | `crates/pi-coding-agent/src/utils/open_browser.rs` | Pure launcher mapping and failure tests |
| `coding-agent/src/utils/photon.ts` | 139 | 0 | todo |  |  |
| `coding-agent/src/utils/pi-user-agent.ts` | 4 | 0 | done | `crates/pi-coding-agent/src/utils/pi_user_agent.rs` | Differential 26005; runtime token is `rust` |
| `coding-agent/src/utils/sleep.ts` | 18 | 0 | done | `crates/pi-coding-agent/src/utils/sleep.rs` | Focused timing/abort tests |
| `coding-agent/src/utils/terminal-current-directory.ts` | 13 | 0 | done | `crates/pi-coding-agent/src/utils/terminal_current_directory.rs` | Differential 26006; focused tests |
| `coding-agent/src/core/detached-job-bootstrap.ts` | 113 | 1 | todo |  |  |
| `coding-agent/src/core/exec.ts` | 107 | 1 | todo |  |  |
| `coding-agent/src/core/permissions/presets.ts` | 62 | 1 | todo |  |  |
| `coding-agent/src/core/tools/output-accumulator.ts` | 224 | 1 | todo |  |  |
| `coding-agent/src/utils/clipboard-image.ts` | 300 | 1 | todo |  |  |
| `coding-agent/src/utils/exif-orientation.ts` | 183 | 1 | todo |  |  |
| `coding-agent/src/utils/git-project.ts` | 13 | 1 | done | `crates/pi-coding-agent/src/utils/git_project.rs` | Real git repository tests |
| `coding-agent/src/utils/paths.ts` | 139 | 1 | done | `crates/pi-coding-agent/src/utils/paths.rs` | Differential 26009 (76 cases); focused tests |
| `coding-agent/src/utils/syntax-highlight.ts` | 146 | 1 | todo |  |  |
| `coding-agent/src/utils/version-check.ts` | 109 | 1 | todo |  |  |
| `coding-agent/src/config.ts` | 595 | 2 | todo |  |  |
| `coding-agent/src/core/model-config.ts` | 298 | 2 | todo |  |  |
| `coding-agent/src/core/tools/path-utils.ts` | 118 | 2 | todo |  |  |
| `coding-agent/src/utils/clipboard.ts` | 175 | 2 | todo |  |  |
| `coding-agent/src/utils/image-convert.ts` | 49 | 2 | todo |  |  |
| `coding-agent/src/utils/image-resize-core.ts` | 164 | 2 | todo |  |  |
| `coding-agent/src/utils/windows-self-update.ts` | 84 | 2 | todo |  |  |
| `coding-agent/src/core/auth-guidance.ts` | 25 | 3 | todo |  |  |
| `coding-agent/src/core/permissions/rule-store.ts` | 303 | 3 | todo |  |  |
| `coding-agent/src/core/tools/edit-diff.ts` | 645 | 3 | todo |  |  |
| `coding-agent/src/core/trust-manager.ts` | 254 | 3 | todo |  |  |
| `coding-agent/src/utils/changelog.ts` | 201 | 3 | todo |  |  |
| `coding-agent/src/utils/image-resize-worker.ts` | 42 | 3 | todo |  |  |
| `coding-agent/src/utils/image-resize.ts` | 123 | 3 | todo |  |  |
| `coding-agent/src/utils/shell.ts` | 225 | 3 | todo |  |  |
| `coding-agent/src/utils/tools-manager.ts` | 365 | 3 | todo |  |  |
| `coding-agent/src/core/remote-catalog-provider.ts` | 132 | 4 | todo |  |  |
| `coding-agent/src/core/resolve-config-value.ts` | 287 | 4 | partial |  |  |
| `coding-agent/src/core/runtime-credentials.ts` | 58 | 4 | todo |  |  |
| `coding-agent/src/utils/image-process.ts` | 119 | 4 | todo |  |  |
| `coding-agent/src/cli/file-processor.ts` | 87 | 5 | todo |  |  |
| `coding-agent/src/client/transcript.ts` | 101 | 5 | todo |  |  |
| `coding-agent/src/core/auth-storage.ts` | 507 | 5 | todo |  |  |
| `coding-agent/src/utils/tool-result-images.ts` | 62 | 5 | todo |  |  |
| `coding-agent/src/core/models-store.ts` | 146 | 6 | todo |  |  |
| `coding-agent/src/bun/register-bedrock.ts` | 4 | 7 | todo |  |  |
| `coding-agent/src/core/provider-composer.ts` | 572 | 7 | todo |  |  |
| `coding-agent/src/extensions/llama/provider.ts` | 150 | 7 | todo |  |  |
| `coding-agent/src/core/keybindings.ts` | 434 | 8 | todo |  |  |
| `coding-agent/src/core/model-runtime.ts` | 795 | 8 | todo |  |  |
| `coding-agent/src/modes/interactive/components/countdown-timer.ts` | 39 | 8 | todo |  |  |
| `coding-agent/src/modes/interactive/components/visual-truncate.ts` | 50 | 8 | todo |  |  |
| `coding-agent/src/modes/interactive/interactive-root-compositor.ts` | 40 | 8 | todo |  |  |
| `coding-agent/src/modes/interactive/working-editor.ts` | 67 | 8 | todo |  |  |
| `coding-agent/src/cli/list-models.ts` | 115 | 9 | todo |  |  |
| `coding-agent/src/cli/login-command.ts` | 278 | 9 | todo |  |  |
| `coding-agent/src/client/remote-session.ts` | 420 | 9 | todo |  |  |
| `coding-agent/src/core/model-registry.ts` | 158 | 9 | todo |  |  |
| `coding-agent/src/migrations.ts` | 365 | 9 | todo |  |  |
| `coding-agent/src/client/index.ts` | 15 | 10 | todo |  |  |
| `coding-agent/src/core/compaction/utils.ts` | 198 | 14 | todo |  |  |
| `coding-agent/src/core/defaults.ts` | 3 | 14 | partial |  |  |
| `coding-agent/src/core/messages.ts` | 212 | 14 | todo |  |  |
| `coding-agent/src/core/runtime-message-markers.ts` | 36 | 14 | todo |  |  |
| `coding-agent/src/core/settings-manager.ts` | 1442 | 14 | todo |  |  |
| `coding-agent/src/core/detached-job-runner.ts` | 167 | 15 | todo |  |  |
| `coding-agent/src/core/multi-agent-store.ts` | 1387 | 15 | todo |  |  |
| `coding-agent/src/core/package-manager.ts` | 2699 | 15 | todo |  |  |
| `coding-agent/src/core/session-control-db.ts` | 7047 | 15 | todo |  |  |
| `coding-agent/src/core/session-manager.ts` | 2437 | 15 | partial |  |  |
| `coding-agent/src/core/session-tool-output.ts` | 380 | 15 | partial |  |  |
| `coding-agent/src/core/telemetry.ts` | 13 | 15 | todo |  |  |
| `coding-agent/extensions/agents-core/src/runtime-utils.ts` | 30 | 16 | todo |  |  |
| `coding-agent/extensions/pyrun/src/detached-bridge.ts` | 153 | 16 | todo |  |  |
| `coding-agent/src/architect/observer.ts` | 159 | 16 | todo |  |  |
| `coding-agent/src/cli/control-command.ts` | 122 | 16 | todo |  |  |
| `coding-agent/src/cli/debug-command.ts` | 77 | 16 | todo |  |  |
| `coding-agent/src/cli/sessions-command.ts` | 106 | 16 | todo |  |  |
| `coding-agent/src/core/cache-stats.ts` | 164 | 16 | todo |  |  |
| `coding-agent/src/core/compaction/compaction.ts` | 1179 | 16 | todo |  |  |
| `coding-agent/src/core/detached-job-cleanup.ts` | 351 | 16 | todo |  |  |
| `coding-agent/src/core/detached-job-control.ts` | 213 | 16 | todo |  |  |
| `coding-agent/src/core/external-session-importer.ts` | 310 | 16 | todo |  |  |
| `coding-agent/src/core/provider-attribution.ts` | 97 | 16 | todo |  |  |
| `coding-agent/src/core/runtime-coordination-format.ts` | 38 | 16 | todo |  |  |
| `coding-agent/src/core/session-cwd.ts` | 83 | 16 | todo |  |  |
| `coding-agent/src/core/session-directory.ts` | 365 | 16 | todo |  |  |
| `coding-agent/src/core/source-info.ts` | 40 | 16 | partial |  |  |
| `coding-agent/src/core/usage-totals.ts` | 67 | 16 | todo |  |  |
| `coding-agent/src/modes/interactive/components/custom-editor.ts` | 148 | 16 | todo |  |  |
| `coding-agent/src/modes/interactive/components/session-selector-search.ts` | 219 | 16 | todo |  |  |
| `coding-agent/src/supervisor/client.ts` | 63 | 16 | todo |  |  |
| `coding-agent/src/supervisor/service.ts` | 210 | 16 | todo |  |  |
| `coding-agent/src/architect/prompt.ts` | 12 | 17 | todo |  |  |
| `coding-agent/src/core/compaction/branch-summarization.ts` | 378 | 17 | todo |  |  |
| `coding-agent/src/core/detached-bash-runner.ts` | 294 | 17 | todo |  |  |
| `coding-agent/src/core/lifecycle-coordinator.ts` | 448 | 17 | todo |  |  |
| `coding-agent/src/core/prompt-templates.ts` | 285 | 17 | todo |  |  |
| `coding-agent/src/core/skills.ts` | 487 | 17 | todo |  |  |
| `coding-agent/src/core/slash-commands.ts` | 48 | 17 | todo |  |  |
| `coding-agent/src/core/terminal-outbox-delivery.ts` | 84 | 17 | todo |  |  |
| `coding-agent/src/modes/interactive/theme/theme.ts` | 1337 | 17 | todo |  |  |
| `coding-agent/extensions/agents-core/src/lifecycle-runtime.ts` | 42 | 18 | todo |  |  |
| `coding-agent/extensions/pyrun/src/detached-runner.ts` | 411 | 18 | todo |  |  |
| `coding-agent/src/core/compaction/index.ts` | 7 | 18 | todo |  |  |
| `coding-agent/src/core/detached-bash-runner-entry.ts` | 13 | 18 | todo |  |  |
| `coding-agent/src/core/detached-job-lifecycle.ts` | 214 | 18 | todo |  |  |
| `coding-agent/src/core/tools/render-utils.ts` | 85 | 18 | todo |  |  |
| `coding-agent/src/modes/interactive/components/agent-selection-banner.ts` | 36 | 18 | todo |  |  |
| `coding-agent/src/modes/interactive/components/armin.ts` | 382 | 18 | todo |  |  |
| `coding-agent/src/modes/interactive/components/daxnuts.ts` | 164 | 18 | todo |  |  |
| `coding-agent/src/modes/interactive/components/diff.ts` | 147 | 18 | todo |  |  |
| `coding-agent/src/modes/interactive/components/dynamic-border.ts` | 8 | 18 | todo |  |  |
| `coding-agent/src/modes/interactive/components/keybinding-hints.ts` | 48 | 18 | todo |  |  |
| `coding-agent/src/modes/interactive/theme/theme-controller.ts` | 139 | 18 | todo |  |  |
| `coding-agent/extensions/pyrun/src/detached-runner-entry.ts` | 13 | 19 | todo |  |  |
| `coding-agent/src/internal-runner.ts` | 19 | 19 | todo |  |  |
| `coding-agent/src/modes/interactive/components/agent-switcher.ts` | 269 | 19 | todo |  |  |
| `coding-agent/src/modes/interactive/components/approval-selector.ts` | 103 | 19 | todo |  |  |
| `coding-agent/src/modes/interactive/components/bash-execution.ts` | 278 | 19 | todo |  |  |
| `coding-agent/src/modes/interactive/components/bordered-loader.ts` | 70 | 19 | todo |  |  |
| `coding-agent/src/modes/interactive/components/branch-summary-message.ts` | 58 | 19 | todo |  |  |
| `coding-agent/src/modes/interactive/components/compaction-summary-message.ts` | 92 | 19 | todo |  |  |
| `coding-agent/src/modes/interactive/components/config-selector.ts` | 942 | 19 | todo |  |  |
| `coding-agent/src/modes/interactive/components/earendil-announcement.ts` | 53 | 19 | todo |  |  |
| `coding-agent/src/modes/interactive/components/extension-editor.ts` | 132 | 19 | todo |  |  |
| `coding-agent/src/modes/interactive/components/extension-input.ts` | 88 | 19 | todo |  |  |
| `coding-agent/src/modes/interactive/components/extension-selector.ts` | 112 | 19 | todo |  |  |
| `coding-agent/src/modes/interactive/components/first-time-setup.ts` | 145 | 19 | todo |  |  |
| `coding-agent/src/modes/interactive/components/login-dialog.ts` | 233 | 19 | todo |  |  |
| `coding-agent/src/modes/interactive/components/model-selector.ts` | 373 | 19 | todo |  |  |
| `coding-agent/src/modes/interactive/components/oauth-selector.ts` | 214 | 19 | todo |  |  |
| `coding-agent/src/modes/interactive/components/sandbox-selector.ts` | 130 | 19 | todo |  |  |
| `coding-agent/src/modes/interactive/components/scoped-models-selector.ts` | 407 | 19 | todo |  |  |
| `coding-agent/src/modes/interactive/components/session-selector.ts` | 1119 | 19 | todo |  |  |
| `coding-agent/src/modes/interactive/components/settings-selector.ts` | 894 | 19 | todo |  |  |
| `coding-agent/src/modes/interactive/components/show-images-selector.ts` | 50 | 19 | todo |  |  |
| `coding-agent/src/modes/interactive/components/theme-selector.ts` | 67 | 19 | todo |  |  |
| `coding-agent/src/modes/interactive/components/thinking-selector.ts` | 76 | 19 | todo |  |  |
| `coding-agent/src/modes/interactive/components/tree-selector.ts` | 1464 | 19 | todo |  |  |
| `coding-agent/src/modes/interactive/components/trust-selector.ts` | 134 | 19 | todo |  |  |
| `coding-agent/src/modes/interactive/components/user-message-selector.ts` | 155 | 19 | todo |  |  |
| `coding-agent/src/cli/config-selector.ts` | 56 | 20 | todo |  |  |
| `coding-agent/src/cli/startup-ui.ts` | 239 | 20 | todo |  |  |
| `coding-agent/src/cli/session-picker.ts` | 96 | 21 | todo |  |  |
| `coding-agent/extensions/agent-viewer/src/index.ts` | 9 | 22 | todo |  |  |
| `coding-agent/extensions/agents-core/src/control-runtime.ts` | 440 | 22 | todo |  |  |
| `coding-agent/extensions/agents-core/src/dispatch-runtime.ts` | 660 | 22 | todo |  |  |
| `coding-agent/extensions/agents-core/src/index.ts` | 34 | 22 | todo |  |  |
| `coding-agent/extensions/agents-core/src/mailbox-runtime.ts` | 518 | 22 | todo |  |  |
| `coding-agent/extensions/agents-core/src/registration.ts` | 312 | 22 | todo |  |  |
| `coding-agent/extensions/agents-core/src/runtime.ts` | 762 | 22 | todo |  |  |
| `coding-agent/extensions/agents-core/src/session-factories.ts` | 497 | 22 | todo |  |  |
| `coding-agent/extensions/agents-core/src/shared.ts` | 54 | 22 | todo |  |  |
| `coding-agent/extensions/agents-core/src/spawn-attach-runtime.ts` | 637 | 22 | todo |  |  |
| `coding-agent/extensions/agents-core/src/tool-results.ts` | 19 | 22 | todo |  |  |
| `coding-agent/extensions/agents-core/src/viewer.ts` | 230 | 22 | todo |  |  |
| `coding-agent/extensions/agents-core/src/wait-runtime.ts` | 403 | 22 | todo |  |  |
| `coding-agent/extensions/agents-mailbox/src/index.ts` | 9 | 22 | todo |  |  |
| `coding-agent/extensions/approval-controls/src/index.ts` | 60 | 22 | todo |  |  |
| `coding-agent/extensions/ask-secret/src/index.ts` | 169 | 22 | todo |  |  |
| `coding-agent/extensions/browser-cli/src/index.ts` | 169 | 22 | todo |  |  |
| `coding-agent/extensions/bwrap/src/backend.ts` | 394 | 22 | todo |  |  |
| `coding-agent/extensions/bwrap/src/index.ts` | 466 | 22 | todo |  |  |
| `coding-agent/extensions/claude-bash-hook/src/index.ts` | 194 | 22 | todo |  |  |
| `coding-agent/extensions/claude-memory-enrich/src/index.ts` | 162 | 22 | todo |  |  |
| `coding-agent/extensions/claude-memory-session-end/src/index.ts` | 61 | 22 | todo |  |  |
| `coding-agent/extensions/codex-fast/src/index.ts` | 148 | 22 | todo |  |  |
| `coding-agent/extensions/codex-image-generation/src/index.ts` | 190 | 22 | todo |  |  |
| `coding-agent/extensions/codex-usage/src/index.ts` | 375 | 22 | todo |  |  |
| `coding-agent/extensions/codex-web-search/src/index.ts` | 209 | 22 | todo |  |  |
| `coding-agent/extensions/debug/src/index.ts` | 29 | 22 | todo |  |  |
| `coding-agent/extensions/default-footer/src/index.ts` | 278 | 22 | todo |  |  |
| `coding-agent/extensions/docs-tree-context/src/index.ts` | 101 | 22 | todo |  |  |
| `coding-agent/extensions/effort/src/index.ts` | 234 | 22 | todo |  |  |
| `coding-agent/extensions/goal/src/completion-scheduling.ts` | 103 | 22 | todo |  |  |
| `coding-agent/extensions/goal/src/empty-response-scheduling.ts` | 60 | 22 | todo |  |  |
| `coding-agent/extensions/goal/src/error-status-scheduling.ts` | 72 | 22 | todo |  |  |
| `coding-agent/extensions/goal/src/goal-agent-end-scheduling.ts` | 40 | 22 | todo |  |  |
| `coding-agent/extensions/goal/src/goal-agent-end.ts` | 50 | 22 | todo |  |  |
| `coding-agent/extensions/goal/src/goal-idle-selection.ts` | 42 | 22 | todo |  |  |
| `coding-agent/extensions/goal/src/goal-parsing.ts` | 77 | 22 | todo |  |  |
| `coding-agent/extensions/goal/src/goal-presentation.ts` | 34 | 22 | todo |  |  |
| `coding-agent/extensions/goal/src/goal-review-evidence.ts` | 85 | 22 | todo |  |  |
| `coding-agent/extensions/goal/src/goal-scheduling.ts` | 304 | 22 | todo |  |  |
| `coding-agent/extensions/goal/src/goal-state.ts` | 117 | 22 | todo |  |  |
| `coding-agent/extensions/goal/src/goal-tool.ts` | 43 | 22 | todo |  |  |
| `coding-agent/extensions/goal/src/goal-turn.ts` | 24 | 22 | todo |  |  |
| `coding-agent/extensions/goal/src/goal-types.ts` | 42 | 22 | todo |  |  |
| `coding-agent/extensions/goal/src/index.ts` | 741 | 22 | todo |  |  |
| `coding-agent/extensions/goal/src/rendering.ts` | 157 | 22 | todo |  |  |
| `coding-agent/extensions/goal/src/supervisor-review.ts` | 63 | 22 | todo |  |  |
| `coding-agent/extensions/goal/src/wait-countdown.ts` | 49 | 22 | todo |  |  |
| `coding-agent/extensions/goal/src/wait-status.ts` | 23 | 22 | todo |  |  |
| `coding-agent/extensions/loop/src/index.ts` | 189 | 22 | todo |  |  |
| `coding-agent/extensions/openai-remote-compact/src/index.ts` | 673 | 22 | todo |  |  |
| `coding-agent/extensions/pyrun/src/detached-evaluation.ts` | 695 | 22 | todo |  |  |
| `coding-agent/extensions/pyrun/src/eval-tool.ts` | 274 | 22 | todo |  |  |
| `coding-agent/extensions/pyrun/src/index.ts` | 625 | 22 | todo |  |  |
| `coding-agent/extensions/run-plan/src/index.ts` | 167 | 22 | todo |  |  |
| `coding-agent/extensions/safe/src/index.ts` | 71 | 22 | todo |  |  |
| `coding-agent/extensions/self-restart/src/index.ts` | 32 | 22 | todo |  |  |
| `coding-agent/extensions/session-archive/src/index.ts` | 26 | 22 | todo |  |  |
| `coding-agent/extensions/session-id/src/index.ts` | 10 | 22 | todo |  |  |
| `coding-agent/extensions/spec-validation/src/index.ts` | 34 | 22 | todo |  |  |
| `coding-agent/src/architect/main.ts` | 308 | 22 | todo |  |  |
| `coding-agent/src/architect/project-spec.ts` | 70 | 22 | todo |  |  |
| `coding-agent/src/cli/args.ts` | 481 | 22 | partial |  |  |
| `coding-agent/src/cli/auth-check.ts` | 73 | 22 | todo |  |  |
| `coding-agent/src/cli/auth-command.ts` | 125 | 22 | partial |  |  |
| `coding-agent/src/cli/credential-print.ts` | 87 | 22 | todo |  |  |
| `coding-agent/src/cli/initial-message.ts` | 43 | 22 | todo |  |  |
| `coding-agent/src/cli/list-extensions.ts` | 113 | 22 | todo |  |  |
| `coding-agent/src/cli/list-tools.ts` | 91 | 22 | todo |  |  |
| `coding-agent/src/cli/project-trust.ts` | 62 | 22 | todo |  |  |
| `coding-agent/src/cli/resident-console-command.ts` | 200 | 22 | todo |  |  |
| `coding-agent/src/core/agent-session-runtime.ts` | 576 | 22 | todo |  |  |
| `coding-agent/src/core/agent-session-services.ts` | 219 | 22 | todo |  |  |
| `coding-agent/src/core/agent-session.ts` | 6073 | 22 | todo |  |  |
| `coding-agent/src/core/bash-executor.ts` | 156 | 22 | todo |  |  |
| `coding-agent/src/core/export-html/index.ts` | 316 | 22 | todo |  |  |
| `coding-agent/src/core/export-html/tool-renderer.ts` | 172 | 22 | todo |  |  |
| `coding-agent/src/core/extensions/index.ts` | 192 | 22 | todo |  |  |
| `coding-agent/src/core/extensions/loader.ts` | 790 | 22 | todo |  |  |
| `coding-agent/src/core/extensions/runner.ts` | 1552 | 22 | todo |  |  |
| `coding-agent/src/core/extensions/types.ts` | 2001 | 22 | todo |  |  |
| `coding-agent/src/core/extensions/wrapper.ts` | 46 | 22 | todo |  |  |
| `coding-agent/src/core/footer-data-provider.ts` | 450 | 22 | todo |  |  |
| `coding-agent/src/core/model-resolver.ts` | 780 | 22 | todo |  |  |
| `coding-agent/src/core/permissions/mcp-permission-prompt.ts` | 250 | 22 | todo |  |  |
| `coding-agent/src/core/permissions/orchestrator.ts` | 39 | 22 | todo |  |  |
| `coding-agent/src/core/project-trust.ts` | 96 | 22 | todo |  |  |
| `coding-agent/src/core/resource-loader.ts` | 1270 | 22 | todo |  |  |
| `coding-agent/src/core/sdk.ts` | 462 | 22 | todo |  |  |
| `coding-agent/src/core/self-restart.ts` | 251 | 22 | todo |  |  |
| `coding-agent/src/core/system-prompt.ts` | 218 | 22 | todo |  |  |
| `coding-agent/src/core/tools/ask-architect.ts` | 95 | 22 | todo |  |  |
| `coding-agent/src/core/tools/ask-questions.ts` | 309 | 22 | todo |  |  |
| `coding-agent/src/core/tools/ask-supervisor.ts` | 89 | 22 | todo |  |  |
| `coding-agent/src/core/tools/bash.ts` | 864 | 22 | todo |  |  |
| `coding-agent/src/core/tools/broadcast.ts` | 115 | 22 | todo |  |  |
| `coding-agent/src/core/tools/change-working-directory.ts` | 145 | 22 | todo |  |  |
| `coding-agent/src/core/tools/channel-post.ts` | 72 | 22 | todo |  |  |
| `coding-agent/src/core/tools/code-index.ts` | 329 | 22 | todo |  |  |
| `coding-agent/src/core/tools/edit.ts` | 443 | 22 | todo |  |  |
| `coding-agent/src/core/tools/end-turn.ts` | 37 | 22 | todo |  |  |
| `coding-agent/src/core/tools/find.ts` | 381 | 22 | todo |  |  |
| `coding-agent/src/core/tools/grep.ts` | 400 | 22 | todo |  |  |
| `coding-agent/src/core/tools/index.ts` | 420 | 22 | todo |  |  |
| `coding-agent/src/core/tools/list-sessions.ts` | 81 | 22 | todo |  |  |
| `coding-agent/src/core/tools/ls.ts` | 326 | 22 | todo |  |  |
| `coding-agent/src/core/tools/read.ts` | 468 | 22 | todo |  |  |
| `coding-agent/src/core/tools/resume-session.ts` | 294 | 22 | todo |  |  |
| `coding-agent/src/core/tools/search-current-session-history.ts` | 196 | 22 | todo |  |  |
| `coding-agent/src/core/tools/tool-definition-wrapper.ts` | 160 | 22 | todo |  |  |
| `coding-agent/src/core/tools/write.ts` | 274 | 22 | todo |  |  |
| `coding-agent/src/extensions/index.ts` | 4 | 22 | todo |  |  |
| `coding-agent/src/extensions/llama/index.ts` | 228 | 22 | todo |  |  |
| `coding-agent/src/extensions/llama/ui.ts` | 542 | 22 | todo |  |  |
| `coding-agent/src/index.ts` | 444 | 22 | partial |  |  |
| `coding-agent/src/main.ts` | 1364 | 22 | todo |  |  |
| `coding-agent/src/modes/index.ts` | 22 | 22 | todo |  |  |
| `coding-agent/src/modes/interactive/components/assistant-message.ts` | 197 | 22 | todo |  |  |
| `coding-agent/src/modes/interactive/components/custom-entry.ts` | 62 | 22 | todo |  |  |
| `coding-agent/src/modes/interactive/components/custom-message.ts` | 113 | 22 | todo |  |  |
| `coding-agent/src/modes/interactive/components/footer.ts` | 279 | 22 | todo |  |  |
| `coding-agent/src/modes/interactive/components/index.ts` | 38 | 22 | todo |  |  |
| `coding-agent/src/modes/interactive/components/markdown-transform.ts` | 29 | 22 | todo |  |  |
| `coding-agent/src/modes/interactive/components/mermaid.ts` | 89 | 22 | todo |  |  |
| `coding-agent/src/modes/interactive/components/skill-invocation-message.ts` | 55 | 22 | todo |  |  |
| `coding-agent/src/modes/interactive/components/tool-execution.ts` | 495 | 22 | todo |  |  |
| `coding-agent/src/modes/interactive/components/user-message.ts` | 70 | 22 | todo |  |  |
| `coding-agent/src/modes/interactive/interactive-mode.ts` | 8489 | 22 | todo |  |  |
| `coding-agent/src/modes/json-event.ts` | 40 | 22 | todo |  |  |
| `coding-agent/src/modes/print-mode.ts` | 217 | 22 | todo |  |  |
| `coding-agent/src/modes/rpc/rpc-client.ts` | 642 | 22 | todo |  |  |
| `coding-agent/src/modes/rpc/rpc-mode.ts` | 846 | 22 | todo |  |  |
| `coding-agent/src/modes/rpc/rpc-types.ts` | 291 | 22 | todo |  |  |
| `coding-agent/src/package-manager-cli.ts` | 889 | 22 | todo |  |  |
| `coding-agent/src/supervisor/approval-reviewer.ts` | 19 | 22 | todo |  |  |
| `coding-agent/src/supervisor/main.ts` | 436 | 22 | todo |  |  |
| `coding-agent/src/supervisor/response-tool.ts` | 48 | 22 | todo |  |  |
| `coding-agent/extensions/agents-core/src/parent-agent-journal.ts` | 110 | 23 | todo |  |  |
| `coding-agent/src/cli.ts` | 25 | 23 | todo |  |  |
| `coding-agent/src/cli/experimental/command-options.ts` | 38 | 23 | todo |  |  |
| `coding-agent/src/core/extensions/extension-runtime.ts` | 103 | 23 | todo |  |  |
| `coding-agent/src/core/index.ts` | 101 | 23 | todo |  |  |
| `coding-agent/src/extensions/multi-agent.ts` | 21 | 23 | todo |  |  |
| `coding-agent/src/modes/interactive/components/status-indicator.ts` | 114 | 23 | todo |  |  |
| `coding-agent/src/rpc-entry.ts` | 13 | 23 | todo |  |  |
| `coding-agent/src/server/create-harness.ts` | 161 | 23 | todo |  |  |
| `coding-agent/src/bun/cli.ts` | 15 | 24 | todo |  |  |
| `coding-agent/src/cli/experimental/commands/client.ts` | 44 | 24 | todo |  |  |
| `coding-agent/src/cli/experimental/commands/pi.ts` | 47 | 24 | todo |  |  |
| `coding-agent/src/cli/experimental/commands/server.ts` | 44 | 24 | todo |  |  |
| `coding-agent/src/cli/experimental/cli.ts` | 7 | 25 | todo |  |  |

## evals (8 files, 1277 lines)

| Source | Lines | Level | Status | Rust | Notes |
|---|---:|---:|---|---|---|
| `evals/src/vitest-evals/artifacts.ts` | 113 | 0 | todo |  |  |
| `evals/src/vitest-evals/harness-table.ts` | 193 | 0 | todo |  |  |
| `evals/src/vitest-evals/summary.ts` | 438 | 0 | todo |  |  |
| `evals/src/vitest-evals/reporter.ts` | 111 | 1 | todo |  |  |
| `evals/src/vitest-evals/setup.ts` | 8 | 1 | todo |  |  |
| `evals/src/pi-harness.ts` | 257 | 23 | todo |  |  |
| `evals/src/extensions.eval.ts` | 140 | 24 | todo |  |  |
| `evals/src/smoke.eval.ts` | 17 | 24 | todo |  |  |
