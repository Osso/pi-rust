# coding-agent image-resize-core traceability

Pinned source: `packages/coding-agent/src/utils/image-resize-core.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Differential proof: request `27103` runs the in-process path on 15 fixtures (dimension limits, byte limits, impossible limits, JPEG/WebP/GIF input, custom and high JPEG quality, degenerate 1px targets, defaults, empty MIME, undecodable bytes) and compares the full `ResizedImage` record, including base64 bytes. Focused tests: `crates/pi-coding-agent/tests/image_resize.rs`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `image-resize-core.ts:5-10` | `ImageResizeOptions` (partial) | `ImageResizeOptions` with `Option` fields, camelCase serde; `None` = omitted |
| `image-resize-core.ts:12-20` | `ResizedImage` record | `ResizedImage`, camelCase serde; differential compares every field |
| `image-resize-core.ts:23-30` | Defaults 2000x2000, 4.5MB base64, quality 80 | `DEFAULT_MAX_WIDTH`/`DEFAULT_MAX_HEIGHT`/`DEFAULT_MAX_BYTES`/`DEFAULT_JPEG_QUALITY`; focused `applies_the_default_limits`, differential `defaults` |
| `image-resize-core.ts:36-43` | Base64 candidate size = encoded string length | `candidate`; differential byte-limit fixtures |
| `image-resize-core.ts:60-66` | Input base64 size `ceil(len/3)*4`; decode; EXIF orientation | `resize_image_in_process`; focused input-intact/undecodable cases |
| `image-resize-core.ts:70-83` | Within all limits → original bytes, original MIME (or `image/<format>` when empty), `wasResized: false` | `resize_image_in_process`; focused `returns_the_original_image_within_all_limits`, `falls_back_to_the_input_mime_type_for_an_empty_one`, `keeps_the_declared_mime_type_verbatim`, differential |
| `image-resize-core.ts:86-96` | Target dimensions with `Math.round` scaling per axis | `target_dimensions`, `scale`; differential dimension/height limits |
| `image-resize-core.ts:98-110` | One resize per iteration, then PNG plus each JPEG quality candidate | `encode_candidates`, `quality_steps`; focused `picks_the_smallest_encoding_and_quality_step`, differential quality fixtures |
| `image-resize-core.ts:112-137` | First candidate with `encodedSize < maxBytes` wins (strict) | `resize_image_in_process`; differential byte-limit/impossible-limit fixtures |
| `image-resize-core.ts:139-150` | 1x1 stop, then `max(1, floor(dimension*0.75))`, stop when unchanged | `next_dimension`; focused `returns_none_when_the_image_cannot_fit`, differential degenerate targets |
| `image-resize-core.ts:151-153` | `null` when nothing fits or decoding/encoding fails | `Option::None`; focused undecodable case |
| `Array.from(new Set([...]))` quality order | `jpegQuality, 85, 70, 55, 40` de-duplicated, order preserved | `quality_steps`; differential custom/high quality |

## Discrepancies

- The source spreads `{...DEFAULT_OPTIONS, ...options}`, so an explicitly `undefined` field would overwrite the default with `undefined` and break the comparisons. The typed Rust API cannot represent that input; `None` means "omitted" and uses the default (typed-core exclusion).
- `resizeImageInProcess` is synchronous in Rust; the source is `async` because of `loadPhoton()`.
- Failures return `Option::None` instead of `null`.

## Proof inventory

- Focused cases: 13.
- Differential request: `27103` (15 fixtures).
