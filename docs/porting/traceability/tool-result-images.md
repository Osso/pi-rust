# coding-agent tool-result-images traceability

Pinned source: `packages/coding-agent/src/utils/tool-result-images.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Differential proof: request `27107` normalizes 7 content fixtures (no images, within limits, oversized, oversized with auto-resize disabled, BMP conversion, undecodable, mixed text order) and compares the `changed` identity flag and the full content array. Focused tests: `crates/pi-coding-agent/tests/tool_result_images.rs`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `tool-result-images.ts:5` | `ToolResultContent = TextContent \| ImageContent` | `TextOrImageContent` (pi-ai), re-exported as `ToolResultContent` |
| `tool-result-images.ts:7-10` | `NormalizeToolResultImagesOptions` (`autoResizeImages` default true) | `NormalizeToolResultImagesOptions`; focused disabled-resize case, differential |
| `tool-result-images.ts:25-27` | No image block → same array reference | `Cow::Borrowed`; focused `returns_the_original_slice_when_there_are_no_image_blocks` |
| `tool-result-images.ts:29-31` | Per block: non-image blocks pass through in order | `normalize_tool_result_images`; focused order-preservation case, differential |
| `tool-result-images.ts:37-47` | Process the base64 payload with `autoResizeImages` only (no resize overrides) | `process_image` call with `resize_options: None`; differential |
| `tool-result-images.ts:48-54` | Processing failure → keep the original block (unlike `read`) | focused `keeps_undecodable_images_instead_of_dropping_tool_output` |
| `tool-result-images.ts:56-59` | Identical data, MIME, and no hints → keep the original block | focused `keeps_already_normalized_blocks_when_nothing_changes`, differential |
| `tool-result-images.ts:61-65` | Otherwise emit the new image block followed by one text block with hints joined by `\n` | focused resized case, differential |
| `tool-result-images.ts:67` | `changed` flag; unchanged → original array | `Cow::Borrowed` vs `Cow::Owned`; differential compares `changed` |

## Discrepancies

- "Same array reference" is modeled as `Cow::Borrowed` so callers can skip rewriting; `Cow::Owned` means the result changed.
- `Buffer.from(data, "base64")` uses `pi_js_compat::base64::buffer_from_base64`, which matches Node's lenient decoding (standard and URL-safe alphabets, stop at `=`); proof is the pi-agent-core differential `base64_decode` request `13100`.

## Proof inventory

- Focused cases: 8.
- Differential request: `27107` (7 fixtures).
