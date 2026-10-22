# estimate — frozen common-pin re-port

Frozen `packages/ai/src/utils/estimate.ts` at `4f9e6f8a4` (114 lines). Rust `utils/estimate.rs`.

| Export / branches | Frozen lines | Proof |
|---|---|---|
| `ContextUsageEstimate`, `calculateContextTokens`: total truthiness / component sum | 3–18 | `estimate::calculate_context_tokens_falls_back_to_the_component_sum`; differential usage fixtures |
| `estimateTextTokens`, text/image content fixed cost and UTF-16 length | 28–42 | `text_estimation_counts_utf16_code_units`, `image_content_costs_the_fixed_estimate`; differential request 4/9 |
| `estimateMessageTokens`: user/toolResult/text/thinking/tool argument JSON | 44–60 | `tool_call_tokens_include_name_and_json_arguments`; differential request 4 |
| last successful nonzero assistant usage: reverse array order, no timestamp filter | 62–70 | `latest_assistant_usage_ignores_timestamp_order`; differential requests 0/1/5/6 |
| array estimate: reported usage + trailing messages, otherwise whole array | 72–86 | differential requests 4–9 |
| `estimateContextTokens`: no prefix/tool adjustments once usage exists; otherwise system and tools | 88–114 | `tool_definitions_are_ignored_when_assistant_usage_exists`, prefix fixture; differential requests 0–3/7 |

Removed old-only timestamp-prefix filter and dynamic-added-tool estimate. Assertions changed only to exact frozen algorithm, corroborated by differential oracle; RED had 2 failures before implementation, GREEN 8 behavioral + 1 differential test (10 scenarios). Commands/revision: `/tmp/pi-rust-repin-delta-ai/ledger.md`, revision `7559ccc1` + estimate slice.

Enumerable AgentTool label/executionMode are now retained by Tool and counted by this whole-object JSON path. Actual frozen comparisons cover four metadata variants, including the shared harness fixture and unpaired label units; [carrier/consumer audit](ai-types.md#enumerable-agenttool-metadata), `tool_metadata` GREEN at eede786f. This does not close the other scalar description/schema gaps below.

## Explicit gaps

- `estimate_text_tokens(&str)` cannot accept lone UTF-16 units directly; content/context paths do retain native `JsString` units.
- Rust `Tool` descriptions/schema currently use scalar/serde JSON fields; arbitrary lossless tool-definition JSON is not established by this matrix.
- Non-serializable arbitrary JS runtime values excluded by typed Rust input decision; finite typed fixture coverage does not prove nonfinite/tool serialization boundaries.

Status partial; no approved deviation claimed. No source bugs identified in current fixtures.
