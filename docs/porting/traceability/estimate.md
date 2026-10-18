# estimate traceability

Source: `packages/ai/src/utils/estimate.ts`.
Rust: `crates/pi-ai/src/utils/estimate.rs`.
Tests: `crates/pi-ai/tests/estimate.rs` (8 behavioral), `crates/pi-ai/tests/differential_estimate.rs` (10 requests vs pinned TS).

## Representation
- `ContextUsageEstimate` is serde `camelCase`; `f64` token counts serialize through `pi_js_compat::number` so integral estimates print like `JSON.stringify`.
- JavaScript `String.length` is UTF-16 code units: `js_len` uses `encode_utf16().count()`.
- `string | (TextContent | ImageContent)[]` content is `TextOrImageInput::{String, Blocks}`.
- `estimateContextTokens(Context | Message[])` is split into `estimate_context_tokens(&Context)` and `estimate_messages_tokens(&[Message])` (the array branch).

## Symbols → proof
| TS | Rust | Test |
|---|---|---|
| `calculateContextTokens` (`totalTokens \|\| sum`) | `calculate_context_tokens` | `calculate_context_tokens_falls_back_to_the_component_sum`, differential 1-2 |
| `estimateTextTokens` | `estimate_text_tokens` | `text_estimation_counts_utf16_code_units` |
| `estimateTextAndImageContentTokens` | `estimate_text_and_image_content_tokens` | `image_content_costs_the_fixed_estimate` |
| `estimateMessageTokens` | `estimate_message_tokens` | `tool_call_tokens_include_name_and_json_arguments`, differential 4 |
| `getLastAssistantUsageInfo` | `last_assistant_usage` | stale-usage tests, differential 0-1, 5-6 |
| `estimateMessages` | `estimate_messages` | same |
| `estimateToolsTokens` | `estimate_tools_tokens` | `context_prefix_adds_system_prompt_and_tool_definitions`, differential 2-3 |
| `estimateContextTokens` | `estimate_context_tokens` | `tools_added_after_the_usage_message_are_counted`, differential 2-3, 7-9 |
| `addedToolNames` filtering after usage | `added_tool_names`, `estimate_added_tools_tokens` | `tools_added_after_the_usage_message_are_counted` |

## Branch notes
- `usage.totalTokens || sum` treats `0` and `NaN` as absent; `f64` NaN is handled explicitly.
- Stale usage: an assistant message is applicable only when its timestamp is not older than the latest prefix timestamp (including inserted compaction summaries).
- `error`/`aborted` assistant messages and zero-usage messages never provide usage.
- Images count as 4800 chars; text/tool content uses UTF-16 length; tool calls count name plus `JSON.stringify(arguments)`.
- Context with usage adds only tools named by `addedToolNames` in tool results after the usage index; without usage it adds system prompt and all tools.

## Gaps
- `safeJsonStringify`'s `undefined` result is unreachable (`serde_json::Value` has no undefined); `[unserializable]` is retained for serialization failures.
- JSON number text can differ for float-typed tool arguments parsed from literals like `1.0`: `serde_json` re-prints `1.0` while JS prints `1`. Typed numeric fields elsewhere use `pi_js_compat::number`; arbitrary `arguments` preserve their parsed representation. Affects only `estimateMessageTokens` character counts for such inputs.

## Discrepancies
- None in behavior exercised by provider tool arguments (integers/fractional values round-trip identically).
