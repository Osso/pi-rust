# coding-agent mime traceability

Pinned source: `packages/coding-agent/src/utils/mime.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Differential proof: request `26010` executes the real pinned module across 33 byte buffers (JPEG, PNG, animated PNG, malformed PNG, GIF, WebP, BMP header variants, truncations, unknown data). Focused tests: `crates/pi-coding-agent/tests/mime.rs`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `mime.ts:3-4` | Sniff limit 4100 bytes; PNG signature | `IMAGE_TYPE_SNIFF_BYTES`, `PNG_SIGNATURE`; focused file test and differential |
| `mime.ts:6-23` | JPEG (reject `0xf7`), PNG, GIF, WebP, BMP detection order | `utils::mime::detect_supported_image_mime_type`; focused format tests, differential `26010` |
| `mime.ts:25-34` | Read up to 4100 bytes from a file | `detect_supported_image_mime_type_from_file`; focused file test |
| `mime.ts:36-40` | PNG requires length ≥ 16, IHDR length 13, and `IHDR` | `is_png`; focused malformed-PNG tests |
| `mime.ts:42-55` | Animated PNG detection stops at `acTL` (true) or `IDAT` (false); invalid chunk stepping ends false | `is_animated_png`; focused animated/malformed cases |
| `mime.ts:57-81` | BMP validates declared size, pixel offset, DIB header size, planes, and bit depth | `is_bmp`; focused BMP matrix, differential |
| `mime.ts:83-116` | Little/big-endian reads and prefix checks | `read_uint16_le`, `read_uint32_be`, `read_uint32_le`, `starts_with*`; differential |

## Discrepancies

- The source file reader is async; the Rust function is synchronous and returns `io::Result`.
- Detection returns `Option<String>` instead of `string | null`.

## Proof inventory

- Focused cases: 8.
- Differential request: `26010`, 33 buffers.
