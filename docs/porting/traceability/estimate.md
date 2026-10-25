# estimate — frozen common-pin re-port

Frozen `packages/ai/src/utils/estimate.ts` at `4f9e6f8a4` (111 lines). Rust `utils/estimate.rs`.

| Export / branches | Frozen lines | Proof |
|---|---|---|
| `ContextUsageEstimate`, `calculateContextTokens`: total truthiness / component sum | 3–19 | `estimate::calculate_context_tokens_falls_back_to_the_component_sum`; differential usage fixtures |
| `estimateTextTokens`, text/image content fixed cost and UTF-16 length | 29–43 | `text_estimation_counts_utf16_code_units`, `image_content_costs_the_fixed_estimate`; differential request 4/9 |
| `estimateMessageTokens`: user/toolResult/text/thinking/tool argument JSON | 21–27, 45–61 | `tool_call_tokens_include_name_and_json_arguments`; differential request 4 |
| last successful nonzero assistant usage: reverse array order, no timestamp filter | 63–72 | `latest_assistant_usage_ignores_timestamp_order`; differential requests 0/1/5/6 |
| array estimate: reported usage + trailing messages, otherwise whole array | 74–88 | differential requests 4–9 |
| `estimateContextTokens`: no prefix/tool adjustments once usage exists; otherwise system and tools | 90–111 | `tool_definitions_are_ignored_when_assistant_usage_exists`, prefix fixture; differential requests 0–3/7 |

Removed old-only timestamp-prefix filter and dynamic-added-tool estimate. Assertions changed only to exact frozen algorithm, corroborated by differential oracle; RED had 2 failures before implementation, GREEN 8 behavioral + 1 differential test (10 scenarios). Commands/revision: `/tmp/pi-rust-repin-delta-ai/ledger.md`, revision `7559ccc1` + estimate slice.

Enumerable AgentTool label/executionMode are now retained by Tool and counted by this whole-object JSON path. Actual frozen comparisons cover four metadata variants, including the shared harness fixture and unpaired label units; [carrier/consumer audit](ai-types.md#enumerable-agenttool-metadata), `tool_metadata` GREEN at eede786f. Current Tool descriptions are `JsString`; schema fields retain the existing serde boundary.

## Numeric JSON regression (2026-10-25)

Ordinary numeric tool-schema bounds/defaults exposed a serialization discrepancy: Rust counted `1.0` where frozen JSON.stringify counts `1`. Actual-source request10 failed at44 versus42 tokens before changing the private serializer to use the existing JS JSON authority. Requests10–11 compare integer-valued floats and ordinary decimal/exponent bounds; no public signatures or carriers change. At81cf614ed, native8 and the source differential1 (12 scenarios) pass, as does the metadata differential1 (four variants). Required two-package all-target check passes; scoped Clippy reports0 errors/9 existing warnings/0 changed-line warnings. Logs: `/tmp/ai-estimate-{current-green,metadata-green,check,clippy}*`. Changed Rust lines have no readability findings; the metrics binary is unavailable. [Contract](../../specs/ai-estimate.md).

## Explicit gaps

Under the 2026-10-25 user-observable parity decision, exotic/non-serializable runtime values, numeric extremes and lone UTF-16 units beyond the existing `&str`/serde schema carriers are recorded gaps; no new carrier is required. Existing `JsString` content, system prompts, descriptions and metadata and lossless argument JSON are retained.

Status done at the approved user-observable parity bar. No frozen source bug identified.
