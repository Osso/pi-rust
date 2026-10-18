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
| `ai/src/api/anthropic-messages.ts` | 1366 | 1 | todo |  |  |
| `ai/src/api/azure-openai-responses.ts` | 330 | 1 | todo |  |  |
| `ai/src/api/bedrock-converse-stream.ts` | 1190 | 1 | todo |  |  |
| `ai/src/api/constrained-sampling.ts` | 277 | 1 | done | `crates/pi-ai/src/api/constrained_sampling.rs` | Strict-schema conversion, grammar sampling, append-only JSON deltas; differential 47 requests. |
| `ai/src/api/github-copilot-headers.ts` | 37 | 1 | done | `crates/pi-ai/src/api/github_copilot_headers.rs` | Initiator/vision/header builders; differential 16 requests. |
| `ai/src/api/google-generative-ai.ts` | 521 | 1 | todo |  |  |
| `ai/src/api/google-shared.ts` | 419 | 1 | todo |  |  |
| `ai/src/api/google-vertex.ts` | 596 | 1 | todo |  |  |
| `ai/src/api/lazy.ts` | 98 | 1 | todo |  |  |
| `ai/src/api/mistral-conversations.ts` | 931 | 1 | todo |  |  |
| `ai/src/api/openai-codex-responses.ts` | 1727 | 1 | todo |  |  |
| `ai/src/api/openai-completions.ts` | 1577 | 1 | todo |  |  |
| `ai/src/api/openai-responses-shared.ts` | 983 | 1 | todo |  |  |
| `ai/src/api/openai-responses.ts` | 372 | 1 | todo |  |  |
| `ai/src/api/pi-messages.ts` | 433 | 1 | todo |  |  |
| `ai/src/api/simple-options.ts` | 97 | 1 | todo |  |  |
| `ai/src/api/transform-messages.ts` | 228 | 1 | done | `crates/pi-ai/src/api/transform_messages.rs` | Cross-model thinking/tool-call normalization, image downgrade, synthetic tool results; injected clock; differential 18 scenarios. |
| `ai/src/auth/context.ts` | 45 | 1 | todo |  |  |
| `ai/src/auth/credential-store.ts` | 67 | 1 | todo |  |  |
| `ai/src/auth/resolve.ts` | 205 | 1 | todo |  |  |
| `ai/src/auth/types.ts` | 240 | 1 | todo |  |  |
| `ai/src/models-store.ts` | 45 | 1 | todo |  |  |
| `ai/src/models.ts` | 953 | 1 | todo |  |  |
| `ai/src/types.ts` | 846 | 1 | partial | `pi-ai/src/types.rs` | data contracts done; option/callback interfaces → provider runtime slice |
| `ai/src/utils/deferred-tools.ts` | 39 | 1 | done | `crates/pi-ai/src/utils/deferred_tools.rs` | Immediate/deferred split with insertion order; differential 6 requests. |
| `ai/src/utils/estimate.ts` | 143 | 1 | done | `crates/pi-ai/src/utils/estimate.rs` | Context/message/tool estimation, UTF-16 lengths, added-tool accounting; differential 10 requests. |
| `ai/src/utils/event-stream.ts` | 110 | 1 | done | `pi-ai/src/utils/event_stream.rs` | differential |
| `ai/src/utils/headers.ts` | 18 | 1 | done | `crates/pi-ai/src/utils/headers.rs` | `Headers.entries()` normalization and provider header filtering; differential 7 requests. |
| `ai/src/utils/node-http-proxy.ts` | 112 | 1 | todo |  |  |
| `ai/src/utils/provider-env.ts` | 52 | 1 | partial | `crates/pi-ai/src/utils/provider_env.rs` | Scoped → process env precedence; Bun `/proc/self/environ` fallback not applicable to native Rust (see traceability). |
| `ai/src/api/anthropic-messages.lazy.ts` | 4 | 2 | todo |  |  |
| `ai/src/api/azure-openai-responses.lazy.ts` | 4 | 2 | todo |  |  |
| `ai/src/api/bedrock-converse-stream.lazy.ts` | 30 | 2 | todo |  |  |
| `ai/src/api/cloudflare-gateway-binding.ts` | 192 | 2 | todo |  |  |
| `ai/src/api/google-generative-ai.lazy.ts` | 4 | 2 | todo |  |  |
| `ai/src/api/google-vertex.lazy.ts` | 4 | 2 | todo |  |  |
| `ai/src/api/mistral-conversations.lazy.ts` | 4 | 2 | todo |  |  |
| `ai/src/api/openai-codex-responses.lazy.ts` | 4 | 2 | todo |  |  |
| `ai/src/api/openai-completions.lazy.ts` | 4 | 2 | todo |  |  |
| `ai/src/api/openai-responses.lazy.ts` | 4 | 2 | todo |  |  |
| `ai/src/api/openrouter-images.ts` | 196 | 2 | todo |  |  |
| `ai/src/api/pi-messages.lazy.ts` | 4 | 2 | todo |  |  |
| `ai/src/auth/helpers.ts` | 59 | 2 | todo |  |  |
| `ai/src/auth/oauth/anthropic.ts` | 364 | 2 | todo |  |  |
| `ai/src/auth/oauth/kimi-coding.ts` | 310 | 2 | todo |  |  |
| `ai/src/auth/oauth/load.ts` | 68 | 2 | todo |  |  |
| `ai/src/auth/oauth/openai-codex.ts` | 544 | 2 | todo |  |  |
| `ai/src/auth/oauth/openrouter.ts` | 311 | 2 | todo |  |  |
| `ai/src/auth/oauth/xai.ts` | 239 | 2 | todo |  |  |
| `ai/src/bedrock-provider.ts` | 6 | 2 | todo |  |  |
| `ai/src/compat/extension-oauth-types.ts` | 45 | 2 | todo |  |  |
| `ai/src/env-api-keys.ts` | 187 | 2 | todo |  |  |
| `ai/src/image-models.generated.ts` | 639 | 2 | todo |  |  |
| `ai/src/images-api-registry.ts` | 53 | 2 | todo |  |  |
| `ai/src/images-models.ts` | 275 | 2 | todo |  |  |
| `ai/src/model-catalog.ts` | 27 | 2 | done | `crates/pi-ai/src/model_catalog.rs` | Group flattening; `provider` parameter is type-only; differential 4 requests. |
| `ai/src/model-types.ts` | 273 | 2 | done | `crates/pi-ai/src/types.rs` | Type-only module; all interfaces mapped to existing `types.rs` items, no new code. |
| `ai/src/providers/cloudflare-auth.ts` | 103 | 2 | todo |  |  |
| `ai/src/providers/cloudflare-stream.ts` | 28 | 2 | todo |  |  |
| `ai/src/providers/faux.ts` | 708 | 2 | todo |  |  |
| `ai/src/providers/radius-config.ts` | 96 | 2 | todo |  |  |
| `ai/src/utils/overflow.ts` | 181 | 2 | done | `crates/pi-ai/src/utils/overflow.rs` | 26 overflow patterns, non-overflow exclusions, silent and length-stop overflow; differential 40 checks + pattern matrix. |
| `ai/src/utils/retry.ts` | 253 | 2 | done | `crates/pi-ai/src/utils/retry.rs` | Exact classifier plus async backoff/cancellation/callbacks; differential 45 classifications + 10 scripted loops. |
| `ai/src/utils/text.ts` | 12 | 2 | done | `crates/pi-ai/src/utils/text.rs` | `contentText` over assistant and tool-result blocks; differential 6 requests. |
| `ai/src/utils/validation.ts` | 350 | 2 | done | `crates/pi-ai/src/utils/validation.rs` | Full hand-written TypeBox-compatible validator, coercions, error messages; 25 behavioral + 67 differential cases ([traceability](traceability/validation.md)). |
| `ai/src/api/openrouter-images.lazy.ts` | 10 | 3 | todo |  |  |
| `ai/src/auth/oauth/radius.ts` | 403 | 3 | todo |  |  |
| `ai/src/image-models.ts` | 42 | 3 | todo |  |  |
| `ai/src/images.ts` | 21 | 3 | todo |  |  |
| `ai/src/index.ts` | 47 | 3 | todo |  |  |
| `ai/src/legacy-api-aliases.ts` | 108 | 3 | todo |  |  |
| `ai/src/oauth.ts` | 10 | 3 | todo |  |  |
| `ai/src/providers/amazon-bedrock.models.ts` | 8 | 3 | todo |  |  |
| `ai/src/providers/ant-ling.models.ts` | 8 | 3 | todo |  |  |
| `ai/src/providers/anthropic.models.ts` | 8 | 3 | todo |  |  |
| `ai/src/providers/azure-openai-responses.models.ts` | 8 | 3 | todo |  |  |
| `ai/src/providers/baseten.models.ts` | 8 | 3 | todo |  |  |
| `ai/src/providers/cerebras.models.ts` | 8 | 3 | todo |  |  |
| `ai/src/providers/cloudflare-ai-gateway.models.ts` | 8 | 3 | todo |  |  |
| `ai/src/providers/cloudflare-workers-ai.models.ts` | 8 | 3 | todo |  |  |
| `ai/src/providers/deepseek.models.ts` | 8 | 3 | todo |  |  |
| `ai/src/providers/fireworks.models.ts` | 8 | 3 | todo |  |  |
| `ai/src/providers/github-copilot.models.ts` | 8 | 3 | todo |  |  |
| `ai/src/providers/google-vertex.models.ts` | 8 | 3 | todo |  |  |
| `ai/src/providers/google.models.ts` | 8 | 3 | todo |  |  |
| `ai/src/providers/groq.models.ts` | 8 | 3 | todo |  |  |
| `ai/src/providers/huggingface.models.ts` | 8 | 3 | todo |  |  |
| `ai/src/providers/images/register-builtins.ts` | 50 | 3 | todo |  |  |
| `ai/src/providers/kimi-coding.models.ts` | 8 | 3 | todo |  |  |
| `ai/src/providers/minimax-cn.models.ts` | 8 | 3 | todo |  |  |
| `ai/src/providers/minimax.models.ts` | 8 | 3 | todo |  |  |
| `ai/src/providers/mistral.models.ts` | 8 | 3 | todo |  |  |
| `ai/src/providers/moonshotai-cn.models.ts` | 8 | 3 | todo |  |  |
| `ai/src/providers/moonshotai.models.ts` | 8 | 3 | todo |  |  |
| `ai/src/providers/nvidia.models.ts` | 8 | 3 | todo |  |  |
| `ai/src/providers/openai-codex-gc.models.ts` | 8 | 3 | todo |  |  |
| `ai/src/providers/openai-codex.models.ts` | 8 | 3 | todo |  |  |
| `ai/src/providers/openai.models.ts` | 8 | 3 | todo |  |  |
| `ai/src/providers/opencode-go.models.ts` | 8 | 3 | todo |  |  |
| `ai/src/providers/opencode.models.ts` | 8 | 3 | todo |  |  |
| `ai/src/providers/openrouter.models.ts` | 8 | 3 | todo |  |  |
| `ai/src/providers/qwen-token-plan-cn.models.ts` | 8 | 3 | todo |  |  |
| `ai/src/providers/qwen-token-plan-individual.models.ts` | 8 | 3 | todo |  |  |
| `ai/src/providers/qwen-token-plan.models.ts` | 8 | 3 | todo |  |  |
| `ai/src/providers/radius.ts` | 82 | 3 | todo |  |  |
| `ai/src/providers/together.models.ts` | 8 | 3 | todo |  |  |
| `ai/src/providers/vercel-ai-gateway.models.ts` | 8 | 3 | todo |  |  |
| `ai/src/providers/xai.models.ts` | 8 | 3 | todo |  |  |
| `ai/src/providers/xiaomi-token-plan-ams.models.ts` | 8 | 3 | todo |  |  |
| `ai/src/providers/xiaomi-token-plan-cn.models.ts` | 8 | 3 | todo |  |  |
| `ai/src/providers/xiaomi-token-plan-sgp.models.ts` | 8 | 3 | todo |  |  |
| `ai/src/providers/xiaomi.models.ts` | 8 | 3 | todo |  |  |
| `ai/src/providers/zai-coding-cn.models.ts` | 8 | 3 | todo |  |  |
| `ai/src/providers/zai.models.ts` | 8 | 3 | todo |  |  |
| `ai/src/auth/oauth/github-copilot.ts` | 417 | 4 | todo |  |  |
| `ai/src/models.generated.ts` | 127 | 4 | todo |  |  |
| `ai/src/providers/amazon-bedrock.ts` | 90 | 4 | todo |  |  |
| `ai/src/providers/ant-ling.ts` | 15 | 4 | todo |  |  |
| `ai/src/providers/anthropic.ts` | 59 | 4 | todo |  |  |
| `ai/src/providers/azure-openai-responses.ts` | 14 | 4 | todo |  |  |
| `ai/src/providers/baseten.ts` | 15 | 4 | todo |  |  |
| `ai/src/providers/cerebras.ts` | 15 | 4 | todo |  |  |
| `ai/src/providers/cloudflare-ai-gateway.ts` | 23 | 4 | todo |  |  |
| `ai/src/providers/cloudflare-workers-ai.ts` | 15 | 4 | todo |  |  |
| `ai/src/providers/deepseek.ts` | 15 | 4 | todo |  |  |
| `ai/src/providers/fireworks.ts` | 19 | 4 | todo |  |  |
| `ai/src/providers/github-copilot.ts` | 34 | 4 | todo |  |  |
| `ai/src/providers/google-vertex.ts` | 100 | 4 | todo |  |  |
| `ai/src/providers/google.ts` | 15 | 4 | todo |  |  |
| `ai/src/providers/groq.ts` | 15 | 4 | todo |  |  |
| `ai/src/providers/huggingface.ts` | 15 | 4 | todo |  |  |
| `ai/src/providers/kimi-coding.ts` | 24 | 4 | todo |  |  |
| `ai/src/providers/minimax-cn.ts` | 15 | 4 | todo |  |  |
| `ai/src/providers/minimax.ts` | 15 | 4 | todo |  |  |
| `ai/src/providers/mistral.ts` | 15 | 4 | todo |  |  |
| `ai/src/providers/moonshotai-cn.ts` | 15 | 4 | todo |  |  |
| `ai/src/providers/moonshotai.ts` | 15 | 4 | todo |  |  |
| `ai/src/providers/nvidia.ts` | 15 | 4 | todo |  |  |
| `ai/src/providers/openai-codex.ts` | 25 | 4 | todo |  |  |
| `ai/src/providers/openai.ts` | 15 | 4 | todo |  |  |
| `ai/src/providers/opencode-go.ts` | 20 | 4 | todo |  |  |
| `ai/src/providers/opencode.ts` | 24 | 4 | todo |  |  |
| `ai/src/providers/openrouter-images.ts` | 22 | 4 | todo |  |  |
| `ai/src/providers/openrouter.ts` | 23 | 4 | todo |  |  |
| `ai/src/providers/qwen-token-plan-cn.ts` | 15 | 4 | todo |  |  |
| `ai/src/providers/qwen-token-plan-individual.ts` | 15 | 4 | todo |  |  |
| `ai/src/providers/qwen-token-plan.ts` | 15 | 4 | todo |  |  |
| `ai/src/providers/together.ts` | 15 | 4 | todo |  |  |
| `ai/src/providers/vercel-ai-gateway.ts` | 15 | 4 | todo |  |  |
| `ai/src/providers/xai.ts` | 28 | 4 | todo |  |  |
| `ai/src/providers/xiaomi-token-plan-ams.ts` | 15 | 4 | todo |  |  |
| `ai/src/providers/xiaomi-token-plan-cn.ts` | 15 | 4 | todo |  |  |
| `ai/src/providers/xiaomi-token-plan-sgp.ts` | 15 | 4 | todo |  |  |
| `ai/src/providers/xiaomi.ts` | 15 | 4 | todo |  |  |
| `ai/src/providers/zai-coding-cn.ts` | 15 | 4 | todo |  |  |
| `ai/src/providers/zai.ts` | 15 | 4 | todo |  |  |
| `ai/src/bun-oauth.ts` | 21 | 5 | todo |  |  |
| `ai/src/providers/all.ts` | 159 | 5 | todo |  |  |
| `ai/src/cli.ts` | 119 | 6 | todo |  |  |
| `ai/src/compat.ts` | 299 | 6 | todo |  |  |
| `ai/src/base.ts` | 6 | 7 | todo |  |  |

## agent (47 files, 12044 lines)

| Source | Lines | Level | Status | Rust | Notes |
|---|---:|---:|---|---|---|
| `agent/src/harness/events.ts` | 102 | 0 | todo |  |  |
| `agent/src/harness/result.ts` | 63 | 0 | todo |  |  |
| `agent/src/harness/tools/edit-diff.ts` | 500 | 0 | partial |  |  |
| `agent/src/harness/tools/image.ts` | 104 | 0 | partial |  |  |
| `agent/src/harness/utils/truncate.ts` | 350 | 0 | partial |  |  |
| `agent/src/harness/telemetry.ts` | 615 | 1 | todo |  |  |
| `agent/src/proxy.ts` | 370 | 4 | done | `crates/pi-agent-core/src/agent/proxy.rs` | Proxy SSE reconstruction, error/abort paths, injectable `Fetch`; see `agent-proxy.md`. |
| `agent/src/types.ts` | 470 | 4 | done | `crates/pi-agent-core/src/agent/types.rs` | Full export set; `AgentMessage` custom variant is a raw JSON object; see `agent-types.md`. |
| `agent/src/harness/compaction/utils.ts` | 163 | 5 | partial |  |  |
| `agent/src/harness/messages.ts` | 188 | 5 | partial |  |  |
| `agent/src/harness/session/session.ts` | 340 | 5 | todo |  |  |
| `agent/src/harness/session/types.ts` | 406 | 5 | todo |  |  |
| `agent/src/harness/types.ts` | 320 | 5 | partial |  |  |
| `agent/src/stream-fn.ts` | 20 | 5 | done | `crates/pi-agent-core/src/agent/stream_fn.rs` | Process-global default stream fn; exact missing-default error; see `agent-stream-fn.md`. |
| `agent/src/agent-loop.ts` | 1028 | 6 | done | `crates/pi-agent-core/src/agent/agent_loop.rs` | Event ordering, tool modes, steering/follow-up, termination, abort races, restored tool calls; 32 behavioral tests + 25 differential scenarios; see `agent-loop.md`. |
| `agent/src/harness/env/nodejs.ts` | 695 | 6 | todo |  |  |
| `agent/src/harness/prompt-templates.ts` | 262 | 6 | partial |  |  |
| `agent/src/harness/reducer.ts` | 667 | 6 | todo |  |  |
| `agent/src/harness/session/context.ts` | 100 | 6 | todo |  |  |
| `agent/src/harness/session/jsonl/errors.ts` | 27 | 6 | todo |  |  |
| `agent/src/harness/session/jsonl/types.ts` | 57 | 6 | todo |  |  |
| `agent/src/harness/session/state.ts` | 344 | 6 | todo |  |  |
| `agent/src/harness/skills.ts` | 375 | 6 | todo |  |  |
| `agent/src/harness/system-prompt.ts` | 34 | 6 | todo |  |  |
| `agent/src/harness/tools/file-mutation-queue.ts` | 56 | 6 | todo |  |  |
| `agent/src/harness/tools/path-utils.ts` | 30 | 6 | todo |  |  |
| `agent/src/harness/tools/tool-context.ts` | 6 | 6 | todo |  |  |
| `agent/src/harness/utils/shell-output.ts` | 199 | 6 | partial |  |  |
| `agent/src/search/index.ts` | 32 | 6 | todo |  |  |
| `agent/src/search/scanning.ts` | 176 | 6 | todo |  |  |
| `agent/src/agent.ts` | 626 | 7 | done | `crates/pi-agent-core/src/agent/agent.rs` | Stateful wrapper, queues, lifecycle, listener settlement, continue/reset; 23 tests + 2 differential scenarios; see `agent.md`. |
| `agent/src/harness/compaction/compaction.ts` | 910 | 7 | partial |  |  |
| `agent/src/harness/session/jsonl/codec.ts` | 240 | 7 | todo |  |  |
| `agent/src/harness/session/memory.ts` | 192 | 7 | todo |  |  |
| `agent/src/harness/tools/bash.ts` | 161 | 7 | todo |  |  |
| `agent/src/harness/tools/edit.ts` | 127 | 7 | todo |  |  |
| `agent/src/harness/tools/read.ts` | 144 | 7 | todo |  |  |
| `agent/src/harness/tools/write.ts` | 39 | 7 | todo |  |  |
| `agent/src/harness/session/jsonl/storage.ts` | 277 | 8 | todo |  |  |
| `agent/src/harness/tools/index.ts` | 23 | 8 | todo |  |  |
| `agent/src/harness/session/jsonl/repo.ts` | 247 | 9 | todo |  |  |
| `agent/src/harness/session/jsonl.ts` | 9 | 10 | todo |  |  |
| `agent/src/harness/session/index.ts` | 13 | 11 | todo |  |  |
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
| `tui/src/autocomplete.ts` | 793 | 1 | todo |  |  |
| `tui/src/keybindings.ts` | 325 | 1 | todo |  |  |
| `tui/src/latex.ts` | 1373 | 1 | todo |  |  |
| `tui/src/terminal.ts` | 594 | 1 | todo |  |  |
| `tui/src/word-navigation.ts` | 117 | 1 | todo |  |  |
| `tui/src/tui.ts` | 2534 | 2 | todo |  |  |
| `tui/src/components/alt-screen-flash.ts` | 51 | 3 | todo |  |  |
| `tui/src/components/box.ts` | 137 | 3 | todo |  |  |
| `tui/src/components/dynamic-border.ts` | 15 | 3 | todo |  |  |
| `tui/src/components/image.ts` | 127 | 3 | todo |  |  |
| `tui/src/components/input.ts` | 465 | 3 | todo |  |  |
| `tui/src/components/markdown.ts` | 1027 | 3 | todo |  |  |
| `tui/src/components/select-list.ts` | 229 | 3 | todo |  |  |
| `tui/src/components/spacer.ts` | 28 | 3 | todo |  |  |
| `tui/src/components/text.ts` | 106 | 3 | todo |  |  |
| `tui/src/components/truncated-text.ts` | 65 | 3 | todo |  |  |
| `tui/src/editor-component.ts` | 74 | 3 | todo |  |  |
| `tui/src/layout-node.ts` | 51 | 3 | todo |  |  |
| `tui/src/root-compositor.ts` | 91 | 3 | todo |  |  |
| `tui/src/tui-main-screen.ts` | 586 | 3 | todo |  |  |
| `tui/src/alt-screen-search.ts` | 157 | 4 | todo |  |  |
| `tui/src/components/loader.ts` | 94 | 4 | todo |  |  |
| `tui/src/components/scroll-view.ts` | 216 | 4 | todo |  |  |
| `tui/src/components/settings-list.ts` | 249 | 4 | todo |  |  |
| `tui/src/components/stack.ts` | 154 | 4 | todo |  |  |
| `tui/src/components/cancellable-loader.ts` | 40 | 5 | todo |  |  |
| `tui/src/components/editor.ts` | 2687 | 5 | todo |  |  |
| `tui/src/components/h-stack.ts` | 44 | 5 | todo |  |  |
| `tui/src/components/v-stack.ts` | 33 | 5 | todo |  |  |
| `tui/src/layout.ts` | 410 | 5 | todo |  |  |
| `tui/src/tui-alt-screen.ts` | 1291 | 6 | todo |  |  |
| `tui/src/index.ts` | 161 | 7 | todo |  |  |

## client (10 files, 1225 lines)

| Source | Lines | Level | Status | Rust | Notes |
|---|---:|---:|---|---|---|
| `client/src/promise.ts` | 16 | 0 | todo |  |  |
| `client/src/transport.ts` | 18 | 0 | todo |  |  |
| `client/src/errors.ts` | 56 | 5 | todo |  |  |
| `client/src/types.ts` | 26 | 5 | todo |  |  |
| `client/src/unix.ts` | 156 | 5 | todo |  |  |
| `client/src/connection.ts` | 236 | 6 | todo |  |  |
| `client/src/session-handle.ts` | 111 | 6 | todo |  |  |
| `client/src/state.ts` | 156 | 6 | todo |  |  |
| `client/src/client.ts` | 432 | 7 | todo |  |  |
| `client/src/index.ts` | 18 | 8 | todo |  |  |

## server (13 files, 1839 lines)

| Source | Lines | Level | Status | Rust | Notes |
|---|---:|---:|---|---|---|
| `server/src/errors.ts` | 58 | 5 | todo |  |  |
| `server/src/protocol.ts` | 388 | 5 | todo |  |  |
| `server/src/connection.ts` | 36 | 6 | todo |  |  |
| `server/src/listener.ts` | 10 | 6 | todo |  |  |
| `server/src/types.ts` | 63 | 6 | todo |  |  |
| `server/src/sessions.ts` | 346 | 7 | todo |  |  |
| `server/src/snapshots.ts` | 62 | 7 | todo |  |  |
| `server/src/transports/unix/types.ts` | 15 | 7 | todo |  |  |
| `server/src/server.ts` | 396 | 8 | todo |  |  |
| `server/src/transports/unix/listener.ts` | 434 | 8 | todo |  |  |
| `server/src/index.ts` | 5 | 9 | todo |  |  |
| `server/src/transports/unix/preset.ts` | 23 | 9 | todo |  |  |
| `server/src/transports/unix/index.ts` | 3 | 10 | todo |  |  |

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
