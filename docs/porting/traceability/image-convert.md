# coding-agent image-convert traceability

Pinned source: `packages/coding-agent/src/utils/image-convert.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Differential proof: requests `27101` (base64 payloads) and `27102` (raw bytes) convert PNG, JPEG, WebP, GIF, and BMP fixtures, including undecodable input, and compare base64 output exactly. Focused tests: `crates/pi-coding-agent/tests/image_convert.rs`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `image-convert.ts:4-25` | `convertImageBytesToPng`: decode, apply EXIF orientation, encode PNG, `null` on failure | `convert_image_bytes_to_png`; focused JPEG/raw/undecodable/orientation cases, differential `27102` |
| `image-convert.ts:31-46` | `convertToPng`: PNG passthrough with identical base64, otherwise decode+convert+re-encode | `convert_to_png`; focused passthrough/JPEG cases, differential `27101` |
| `image-convert.ts:8-11` | `loadPhoton()` `null` → `null` | No equivalent: backend is linked (see photon traceability) |
| `image-convert.ts:22-24` | Decode/encode failure → `null` | `Option` from `decode_image`/`png_bytes` errors; focused undecodable cases |

## Discrepancies

- The source is async (`await loadPhoton()`); the Rust functions are synchronous. `processImage` still runs the conversion on a blocking thread.
- The result is a typed `ConvertedImage { data, mime_type }` instead of an inline object.

## Proof inventory

- Focused cases: 5.
- Differential requests: `27101` (8 payloads), `27102` (7 byte fixtures).
