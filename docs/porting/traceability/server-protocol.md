# server/protocol.ts traceability

Source: `packages/server/src/protocol.ts`. Rust: `crates/pi-server/src/protocol.rs`.

| TS export | Rust | Proof |
|---|---|---|
| `toProtocolJsonValue` | `to_protocol_json_value` | `tests/protocol.rs::protocol_json_values_and_diagnostic_details_are_lossless_for_json_input`; `tests/differential_protocol.rs` |
| `sanitizeProtocolDetails` | `sanitize_protocol_details` | same |
| `toProtocolUsage` | `to_protocol_usage` | `tests/protocol.rs::normalizes_usage_counts_and_costs`; differential |
| `toProtocolModelMetadata` | `to_protocol_model_metadata` | `tests/protocol.rs::maps_model_metadata_and_thinking_levels`; differential |
| `toProtocolUserMessage` | `to_protocol_user_message` | `tests/protocol.rs`; differential |
| `toProtocolAssistantMessage` | `to_protocol_assistant_message` | `tests/protocol.rs`; differential |
| `toProtocolToolResultMessage` | `to_protocol_tool_result_message` | `tests/protocol.rs`; differential |

## Branches
- User content string/blocks and text/image parts: `tests/protocol.rs::maps_user_and_tool_messages_without_lossy_details`, `maps_redacted_thinking_and_image_content`.
- Assistant text/thinking(redacted)/toolCall; stop reasons pending/stop/length/toolUse/deferred/error/aborted: `tests/protocol.rs::{maps_every_assistant_content_variant_and_stop_reason, derives_streaming_status_from_a_pending_stop_reason, preserves_optional_non_empty_assistant_error_messages}` plus differential error cases.
- Tool result call id/name mismatch: `tests/protocol.rs::rejects_tool_results_associated_with_a_different_call`.
- Identifier/timestamp validation: `tests/protocol.rs::rejects_invalid_source_identifiers_and_timestamps`.
- Usage count/cost normalization: `tests/protocol.rs::normalizes_usage_counts_and_costs`.
- Model thinking levels: `getSupportedThinkingLevels` is not part of the ported `pi-ai` surface, so `supported_thinking_levels` reimplements its filter (`off` for non-reasoning; `xhigh`/`max`/`ultra` only when mapped; explicit `null` removes a level) and drops `ultra` because protocol v1 has no such literal.

## Differential
`tests/differential_protocol.rs` runs the pinned `packages/server/src/protocol.ts` through `oracle/server-protocol-oracle.ts`, encodes the bridge result with the real TypeScript protocol encoder, and compares canonical CBOR payload hex for values and error messages.

## Gaps / exclusions
- `toProtocolJsonValue`/`sanitizeProtocolDetails` accept `serde_json::Value`, which cannot represent `undefined`, bigint, symbols, functions, non-finite numbers, sparse arrays, `Date`, or circular references. Those lossy-repair branches are unreachable in the typed Rust API and are excluded (August 12, 2026 typed-API decision). JSON inputs are copied losslessly.
- **Discrepancy (pi-protocol):** the Rust protocol serializer emits tool transcript `timestamp` before `status`/`isError`, while the TS schema orders `status`/`isError` first. The differential test canonicalizes CBOR map order before comparing; the bridge values are identical.
