# coding-agent exif-orientation traceability

Pinned source: `packages/coding-agent/src/utils/exif-orientation.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Differential proof: request `27100` applies orientations 1-8 to JPEG (little- and big-endian TIFF) and WebP fixtures and compares width, height, and the full RGBA buffer against the pinned module. Focused tests: `crates/pi-coding-agent/tests/exif_orientation.rs`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `exif-orientation.ts:7-41` | `readOrientationFromTiff`: byte order, IFD offset, entry scan, 1-8 clamp | `read_orientation_from_tiff`; focused LE/BE matrix, out-of-range values, invalid signed IFD offset |
| `exif-orientation.ts:43-72` | `findJpegTiffOffset`: marker walk, APP1 `Exif\0\0` header, segment stepping | `find_jpeg_tiff_offset`; focused fill-marker/empty-segment case, truncated segment, differential |
| `exif-orientation.ts:74-99` | `findWebpTiffOffset`: RIFF chunk walk, optional `Exif\0\0` prefix, even padding | `find_webp_tiff_offset`; focused prefix/no-prefix, padding, empty/odd chunk, truncation |
| `exif-orientation.ts:101-109` | `hasExifHeader` | `has_exif_header`; focused prefix cases |
| `exif-orientation.ts:111-138` | `getExifOrientation`: JPEG `FF D8` / WebP `RIFF....WEBP` dispatch, default 1 | `read_exif_orientation`; focused non-image inputs, differential |
| `exif-orientation.ts:140-160` | `rotate90` with a destination-index callback, dimensions swapped | `rotate_quarter_turn`; focused dimension swap, differential pixel buffers |
| `exif-orientation.ts:163-196` | `applyExifOrientation` for orientations 2-8 (flips mutate, rotations return new) | `apply_exif_orientation` (takes ownership, returns the image); focused per-orientation permutation test, differential |
| JS `Uint8Array` out-of-range reads | `undefined` coerces to 0 in bitwise reads | `byte_at`; focused truncated-segment cases |

## Discrepancies

- **Pinned source bug (fixed upstream):** the pinned revision reads RIFF chunk sizes and little-endian TIFF offsets with `|`/`<<` (signed 32-bit) and advances JPEG/WebP scans without bounding the next offset. A crafted WebP chunk size such as `0xfffffff8` makes `findWebpTiffOffset` loop forever. The port matches upstream Pi `098356ef6` ("bound EXIF chunk scans and use unsigned offsets"): both `read32` results are unsigned, and both scans reject `nextOffset > bytes.length || nextOffset <= offset` (for WebP the check runs before the `EXIF` return). Covered by `upstream_chunk_bounds::*` and `negative_webp_chunk_sizes_do_not_hang`.
- `applyExifOrientation` takes and returns the image by value instead of mutating or returning the same reference; callers no longer free the previous image.
- `getExifOrientation` is exposed as `read_exif_orientation` so the parser branches can be tested directly (the source keeps it module-private).

## Proof inventory

- Focused cases: 16.
- Differential request: `27100` (25 fixtures: per orientation 1-8 one little-endian JPEG, one big-endian JPEG, and one WebP, plus a JPEG without EXIF).
