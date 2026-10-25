# Anthropic messages traceability

Frozen: `packages/ai/src/api/anthropic-messages.ts` at [source lock](../source-lock.json), 1239 lines. Native: `crates/pi-ai/src/api/anthropic_messages.rs`, SDK request helper `anthropic_sdk.rs`, lazy wrapper `anthropic_messages_lazy.rs`.

## Whole-module audit (2026-10-25)

| Source exports / branches | Proof |
|---|---|
| `AnthropicEffort`, `AnthropicThinkingDisplay`, `AnthropicOptions`, `stream`, `streamSimple` | original whole-provider request/event matrix, 26 native behaviors, direct Simple acquisition matrix; native effort retains every mapped string, not enum re-parsing |
| `resolveCacheRetention`, `getCacheControl`, compatibility defaults | explicit/PI_CACHE_RETENTION/default, none, long TTL capability, cache on system/last user/final tool; original source requests and native cache tests |
| Claude Code names/version, `isOAuthToken`, `createClient`, merge/has/auth helpers | API-key, OAuth and Copilot branches; Claude Code2.1.75 identity/tool-name roundtrip; beta ordering, session affinity, model/dynamic/caller overrides and header-owned auth; original full wire fixtures |
| `buildParams`, `convertMessages`, `convertContentBlocks`, tool-result grouping, `convertTools` | exact ordered body bytes; user text/blocks/empty/image filtering, text/thinking/redacted/tool replay, signature capability, consecutive results, source plain properties/required schemas and eager/cache flags; no old strict/deferred/tool-reference paths |
| `normalizeToolCallId` | native per-UTF16-unit replacement then64-unit cap; new astral replay ID/source request regression was RED before the fix |
| thinking/temperature/metadata/tool choice, `mapThinkingLevelToEffort` | adaptive/budget/disabled/off-null capability, custom native effort mappings, shared budget/context clamp, temperature restrictions, user_id and named/default choices; original request matrix |
| SDK request retries and per-attempt headers | actual SDK maxRetries; native ignores source-unused maxRetryDelayMs; new2ms-hint/1ms-old-cap regression. [Shared current SDK hint proof](provider-retry.md) covers the zero-ms/seconds precedence in both actual SDKs |
| `flushSseEvent`, `decodeSseLine`, line splitting, `iterateSseMessages`, `iterateAnthropicEvents` | actual-source stream matrix, unknown/comment/error events, repaired JSON, trailing EOF and required message_stop; new initial-BOM-in-own-chunk regression preserves first ID/usage/events |
| message start/usage, text/thinking/redacted/tool start, all four delta kinds, block stop/scratch cleanup, message_delta | full push-time snapshots and result equality plus native stream tests; native owns index/partialJson exactly until source removes them; human text/args/errors use existing native carriers |
| `mapStopReason`, final/catch | stop/length/toolUse/pause/sequence/refusal/sensitive/unknown; new sensitive regression removes invented native text; new refusal explanation preserves raw unmatched UTF16 in existing errorMessage carrier; raw callback catch matrix and cleanup retained |
| lazy factory | [shared lazy proof](api-lazy.md), native wrapper dispatch |

**DISCREPANCY preserved:** source initializes text/thinking/signature start blocks to empty even when content_block_start supplies nonempty values. Native follows that source behavior; the existing full-content source regression remains intact.

## Explicit open/gap rows

| Status | Boundary |
|---|---|
| open | Source options.client can bypass construction with an injected SDK client such as AnthropicVertex. Native has no equivalent injected-client field. This is an unported branch, not an inferred approval. |
| open proof | Shared SDK request reason/deadline matrices are proven, but a full provider-level real-socket post-header/pending-body rejection reason matrix remains unproven; do not infer it from callback errors or pre-abort fixtures. See [SDK boundary](openai-sdk.md#shared-sdk-reasondeadline-boundary). |
| gap | Missing response bodies have no typed native HttpResponse representation. Wrong-typed/exotic runtime JSON, numeric extremes, OS/runtime fault shapes and unmatched units beyond existing opaque/schema carriers use the project bar. No new carrier is added for those shapes. |

Current renewal: 23 affected source/SDK tests (one separately invoked diagnostics child), including the original provider matrix and five new Anthropic cases; 26 native Anthropic behaviors. [Exact ledger](../ai-api-repin-proof.md). The first incorrectly filtered behavior command ran zero tests and is not evidence. Approved immutable snapshots/native ownership/fixed native message key order remain unchanged. The Rust-only request.fetch seam is used only for bounded fake-fetch proof, never a live provider.
