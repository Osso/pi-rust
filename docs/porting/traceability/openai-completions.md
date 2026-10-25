# OpenAI Completions traceability

Frozen: `packages/ai/src/api/openai-completions.ts` at [source lock](../source-lock.json),1293 lines. Native: `crates/pi-ai/src/api/openai_completions.rs`; shared request/SSE path in `openai_sdk.rs`.

## Whole-module audit (2026-10-25)

| Source symbols / branches | Native proof |
|---|---|
| `OpenAICompletionsOptions`, `hasHeader`, `getClientApiKey`, `hasToolHistory`, cache retention | literal two-field options; header-owned/missing auth; direct Simple Result; context-history empty tools; original35 source scenarios and common acquisition/callback matrices |
| `detectCompat`, `getCompat`: provider/URL flags, six max_tokens families, developer/reasoning/store/usage/strict/cache/affinity defaults and nullish overrides | native raw compat read policies; existing provider-family request matrix, compat unit tests and model-registry request fixtures; [prior default-token proof](../q15-sdk-provider-proof.md) |
| `convertMessages`: nonempty system prompt, developer role, user strings/blocks/images, assistant text/thinking/signature remap, plain content, tool calls/encrypted details, empty-turn suppression | original source request contexts; empty-system and replay regression; encrypted JSON uses lossless parser while invalid signatures remain dropped as source does |
| tool IDs: pipe prefix only, per-UTF16-unit ASCII replacement,40-unit cap; bare OpenAI ID40-unit cap, other providers unchanged | three actual-source pipe/astral/bare-ID cases; removed old combined-item/hash behavior |
| consecutive tool results, optional name, text/placeholder, supported image aggregation, assistant bridge and last-role handling | original rich-message/image cases and new empty tool-result source fixture |
| `convertTools`, cache-control helpers | function-only unmodified schema/native description; strict:false when supported; first instruction/last tool/last text-bearing conversation cache points; source scenarios17/29 |
| `buildParams`: ordered model/messages/stream/cache, usage/store/maxTokens/temperature, tools before tool_stream, cache mutation, toolChoice | full request JSON recursively compared with key order; original scenario9 was RED for reversed tools/tool_stream |
| thinking formats: zai, qwen, qwen-chat-template, chat-template variables/omitWhenOff, deepseek, openrouter, ant-ling, together, string-thinking and generic mapped/off effort | existing source scenarios1–15/31–32; native mapping uses source nullish vs strict-string policies; no old Baseten/sampling/token-budget additions |
| raw OpenRouter routing and Vercel only/order fields | source scenario21 plus native registry raw-order fixtures |
| `streamSimple`: synchronous auth, base/context clamp, model/standard reasoning clamp, caller toolChoice | source Simple regression (old native dropped toolChoice); removed old thinkingBudgets forwarding; SDK retry ignores unrelated maxRetryDelayMs |
| stream state: response ID/model first nonempty, chunk/choice usage, finish-reason map, one text/thinking block, reasoning field precedence/opencode-go remap | original stream matrix; source end/refusal/network/error/EOF cases |
| tool blocks indexed by JS-number/ID, late IDs/names, pending encrypted detail, empty/fragment arguments and scratch/end/catch ordering | four fractional/interleaving/open-error/EOF/retired-custom-input cases; partialArgs/streamIndex are native block authority |
| usage: cache read fallback, separate writes, clamped input, reasoning included in output, cost and totals | source usage scenarios0/18 and native tests |
| catch: scratch cleanup, abort flag, full provider normalization, conditional raw metadata suffix |40-case raw/native callback matrix across8 APIs; new Completions raw metadata includes newline/unmatched units; shared SDK HTTP/error matrix |
| SDK request acquisition, attempt counts, SSE parsing and default malformed-data console diagnostics | shared actual-source SDK6.26 wire tests and [bounded process proof](openai-responses.md#shared-sdk-sse-diagnostics); approved lazy factory uses [lazy wrapper](api-lazy.md) |

**DISCREPANCY preserved:** an empty tool result with no image still emits `(see attached image)` (frozen1037), not native's old `(no tool output)`. No source bug is silently corrected.

## Final-gate stale assertion (2026-10-25)

Commit `22a70abd9` restored frozen pipe-prefix extraction but retained the pre-repin `tool_call_ids_preserve_item_uniqueness` assertion introduced in `04eeff7b8`. Frozen `openai-completions.ts:855–867` explicitly takes `id.split("|")[0]`, sanitizes it, and clips to 40 units; it does not preserve item uniqueness. The unit test now asserts exact `call_1` for both item suffixes and the exact clipped long prefix. Production is unchanged. Existing actual-source replay differential passes (`/tmp/pi-repin-final-gate/ai-replay-frozen.log`).

## Explicit gaps

- Pathological/wrong-typed JSON, numeric extremes, OS/runtime fault shapes and unmatched units beyond existing opaque/schema carriers follow the user-observable bar. Opaque ID clipping that creates an unmatched unit rejects explicitly through the shared uncaught authority; no lossy replacement or new normalization API is introduced.
- Native events use approved immutable push-time partial snapshots; Rust-only fetch injection is retained, and unset uses the default HTTP path.

Proof ledger: [AI API re-pin proof](../ai-api-repin-proof.md). Earlier ingress/error-only evidence does not substitute for this module audit or claim workspace acceptance.
