# text traceability

Source: `packages/ai/src/utils/text.ts`.
Rust: `crates/pi-ai/src/utils/text.rs`.
Tests: `crates/pi-ai/tests/text.rs` (4 behavioral), `crates/pi-ai/tests/differential_text.rs` (6 requests vs pinned TS).

## Representation
- `content: string | Content[]` → `ContentText::{String, Assistant, TextOrImage}` so both assistant and tool-result block types are accepted without a shared trait.
- `separator` is a required parameter; TypeScript's `"\n"` default is supplied by callers.

## Symbols → proof
| TS | Rust | Test |
|---|---|---|
| `contentText` string passthrough | `ContentText::String` | `passes_string_content_through` |
| assistant text filtering | `assistant_block_text` | `extracts_assistant_text_blocks_with_default_separator` |
| custom separator | `join_text_blocks` | `supports_custom_separators` |
| tool-result text filtering | `text_or_image_block_text` | `extracts_text_from_tool_result_content` |
| default separator | not applicable | differential requests pass explicit separators and the omitted case (`\n`) |

## Gaps
- No default argument; callers pass `"\n"` explicitly.

## Discrepancies
- None.
