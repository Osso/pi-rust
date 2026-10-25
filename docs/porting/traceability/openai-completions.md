# openai-completions traceability

## Q15 default-token-field revalidation

At a12f44de, actual frozen DeepSeek dispatch sends max_completion_tokens while native sends max_tokens. Frozen :1214–1215 selects max_tokens only for Chutes/Moonshot/Cloudflare Gateway/Together/Nvidia/AntLing; DeepSeek and Zai are not included. Native removes those two old-only default branches; explicit model compat overrides remain unchanged. Complete public-factory request differential GREEN and focused compat unit GREEN are recorded in the [Q15 proof](../q15-sdk-provider-proof.md).

## Q4 common-pin status

Partial. Corrected global-fetch oracle now drives frozen SDK requests. SDK header6.26.0 restored; removed source-absent rawStopReason assignment after exact scenario0 event/result key RED (`differential_openai_completions-sdk626.log`). Current recorded matrix (35 scenarios) and9 behavioral tests pass at `56a4113a` (`completions-budget-green.log`). Restored frozen request behavior: no Baseten chat_template_args, no Qwen reasoning_effort, function-only tools with strict:false, no sampling-param merge/deferred placement/thinking-token-budget, uniform affinity headers, incremented SDK retry count. These are exact request/event/result comparisons, not whole-module parity. Old public grammar/message-conversion options remain to remove; source-absent stream custom-input handling is removed by the native scratch slice below. Option/compat type cleanup remains pending. [Exact audit/proof](../ai-scripted-provider-oracles.md). Historical recorded-fetch parity claims below are superseded. No assertion narrowed; `request.fetch` is an approved Rust-only seam; unset must match frozen default fetch (`AGENTS.md`).

Q18 human-error representation and bounded proof: [lossless provider errors](lossless-provider-errors.md). Earlier counts are historical, not rerun evidence.

Q18 ingress representation and current bounded proof: [lossless provider ingress](lossless-provider-ingress.md). Earlier suite counts below are historical, not new-pin rerun evidence.

Source: `packages/ai/src/api/openai-completions.ts` and `openai-completions.lazy.ts`.
Rust: `crates/pi-ai/src/api/openai_completions.rs`.
Tests: module unit tests, `crates/pi-ai/tests/openai_completions.rs` (9 behavioral), `crates/pi-ai/tests/differential_openai_completions.rs` (35 recorded-fetch scenarios).

## Native Completions scratch

Frozen `openai-completions.ts:200–242,278–317,405–420,459–465` carries `partialArgs` and numeric `streamIndex` on tool blocks. Native ToolCall fields now retain them in start/delta snapshots, accumulating and parsing one argument buffer. Finalization parses/removes both before toolcall_end; provider catches remove both before error/result. Removed private ToolState's alternate buffers and source-absent custom-input streaming. Function-only source ignores old custom input fields; a direct frozen scenario proves that behavior.

The old integer-only stream lookup rejected fractional indices, creating14 events instead of frozen10 for interleaved0.5/0.75 indices (`completions-native-scratch-red.log`). Lookup now keys exact binary64 bits, with signed zero sharing a key as JS Map requires; the observable streamIndex retains its original f64. No number narrowing is used. New four-case actual-source test covers fractional interleaving, SDK error during open tools, EOF without finish_reason and retired custom input. Existing35 scenarios plus four new cases and9 behavioral tests GREEN (`completions-native-scratch-green-1.log`). [Contract](../../specs/content-stream-scratch.md).

## Symbols → proof
| TS | Rust | Test |
|---|---|---|
| `OpenAICompletionsOptions` | `OpenAICompletionsOptions` | differential 1-3, 31-32 |
| `ConvertCompletionsMessagesOptions` | same | behavioral grammar test |
| `ResolvedOpenAICompletionsCompat` | same | compat unit test |
| `resolveCacheRetention` | `resolve_cache_retention` | 18 |
| `stream` / `streamSimple` | same + `OpenAICompletionsApi` | all |
| `createClient` (copilot, affinity headers) | `create_client` | 19-20 |
| `buildParams` | `build_params` | all request comparisons |
| `buildChatTemplateValues` / `resolveChatTemplateKwargValue` | same-named snake_case | 11-12 |
| `getCompatCacheControl` | `get_compat_cache_control` | 29 |
| `applyAnthropicCacheControl` | `apply_anthropic_cache_control` | 29; behavioral test |
| `addCacheControlToSystemPrompt` | inline in `apply_anthropic_cache_control` | 29 |
| `addCacheControlToLastConversationMessage` | inline in `apply_anthropic_cache_control` | 29 |
| `addCacheControlToLastTool` | inline in `apply_anthropic_cache_control` | 29 |
| `addCacheControlToTextContent` | `add_cache_control_to_text_content` | 29 |
| `convertMessages` | `convert_messages` | behavioral tests; differential 0-30 |
| `convertTools` | `convert_tools` | 17 |
| `parseChunkUsage` | `parse_chunk_usage` | 0, 18 |
| `mapStopReason` | `map_stop_reason` | 16; unit test |
| `detectCompat` / `getCompat` | `detect_compat` / `get_compat` | all scenarios; unit test |
| lazy wrapper | `open_ai_completions_api` | `LazyApi` covered by `api_lazy` tests |

## Branch coverage
- Thinking formats: `openai` (1-3), `openrouter` (3-4), `deepseek` (5-6), `together` (7), `baseten` (8), `zai` (9), `qwen` (10), `chat-template` (11-12), `qwen-chat-template` (13), `string-thinking` (14), `ant-ling` (15).
- Thinking token budget: 31 (custom cap) and 32 (ceiling clamp).
- Tool calls with streamed arguments: 16; grammar/custom tools: 17; strict mode: 17.
- Usage/cost: 0, 18; reasoning tokens: 0.
- Prompt cache key/retention: 18; session affinity: 19-20; developer role: 0 vs 30.
- `max_tokens` vs `max_completion_tokens`: 5-6 vs 0.
- `store` and `stream_options.include_usage`: 0, 33.
- OpenRouter/Vercel routing: 21.
- HTTP errors/retry/abort/malformed: 22-25.
- Images (user + tool result): 26; Kimi deferred tools: 27; encrypted reasoning details: 28.
- Anthropic cache control: 29.

## Differential normalization
- Timestamps become `0`; environment identity headers become `<env>`; malformed-JSON messages are the V8 `SyntaxError` text from `pi_js_compat::js_json` and are compared byte-for-byte; event `partial` is snapshotted at push time.

## Gaps
- `streamSimple` API-key failure returns an error stream instead of throwing synchronously.
- `onPayload`/`onResponse` are exercised by Rust behavior but not serialized through the oracle.
- `as any` request fields (`stream_options`, `thinking`, `chat_template_*`, `thinking_token_budget`) are emitted as JSON values by design.

## Discrepancies
- None. `mapStopReason(null)` is represented by an absent/`null` finish reason and maps to `stop`.

Q18 shared UTF-16 fields, argument parser and outbound-tree migration: [boundary/proof matrix](lossless-content.md). Checkpoint evidence does not establish unchanged consumer-fixture compatibility or full workspace parity.

## Q18 provider request integration (2026-10-22)

Client key selection, default/model/header merges and affinity session IDs stay native. SDK request construction owns URL/header conversion; no affinity-value UTF-8 projection remains.

[Focused public-provider runtime proof](lossless-sdk-boundaries.md) records exact covered cases, revisions and remaining acceptance limits.

## Direct Simple acquisition boundary (2026-10-25)

`differential_simple_acquisition` invokes actual frozen modules and native ProviderStreams for missing/empty API keys across six APIs. All twelve cases were genuine RED: native returned a stream instead of rejecting acquisition. Direct stream_simple now returns Result and adapters propagate it; lazy acquisition continues through its existing catch. Mechanical consumer changes preserve valid-stream assertions. The remaining module audit is separate from this bounded proof.
