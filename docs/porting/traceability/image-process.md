# coding-agent image-process traceability

Pinned source: `packages/coding-agent/src/utils/image-process.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Differential proof: request `27106` processes 8 fixtures (BMP with and without auto-resize, PNG within limits, PNG resized, `IMAGE/JPG; charset=binary`, convertible unsupported MIME, undecodable, impossible size limit) and compares `ok`, `data`, `mimeType`, and `hints` exactly. Focused tests: `crates/pi-coding-agent/tests/image_process.rs`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `image-process.ts:5-11` | `ProcessImageOptions` (`autoResizeImages` default true, `resizeOptions`) | `ProcessImageOptions`; focused hint/limit cases, differential |
| `image-process.ts:13-26` | Discriminated `ProcessImageResult` | `ProcessImageResult::{Processed, Omitted}`; differential serializes the same wire shape |
| `image-process.ts:34-36` | `baseMimeType`: strip parameters, trim, lowercase | `base_mime_type`; focused MIME-alias matrix, differential `IMAGE/JPG; charset=binary` |
| `image-process.ts:38-52` | Supported MIME normalization (`png`, `jpeg`/`jpg`, `gif`, `webp`) | `normalize_supported_image_mime_type`; focused aliases, differential |
| `image-process.ts:54-71` | Unsupported MIME → PNG conversion, `convertedFrom` | `normalize_image`; focused BMP cases, differential |
| `image-process.ts:73-76` | Conversion hint `[Image converted from X to Y.]`, skipped when equal | `conversion_hint`; focused both-hints case, differential |
| `image-process.ts:79-87` | Conversion failure → `[Image omitted: could not be converted to a supported inline image format.]` | `ProcessImageResult::Omitted`; focused undecodable case |
| `image-process.ts:89-95` | Resize failure → `[Image omitted: could not be resized below the inline image size limit.]` | `ProcessImageResult::Omitted`; focused impossible-limit case |
| `image-process.ts:97-106` | Resize path: conversion hint then dimension note | `process_image_blocking`; focused resized/converted cases, differential |
| `image-process.ts:108-120` | No-resize path: base64 of normalized bytes plus conversion hint | `process_image_blocking`; focused `converts_bmp_to_png_when_auto_resize_is_disabled`, differential |

## Discrepancies

- The whole pipeline runs inside one `spawn_blocking` task. The source awaits `convertImageBytesToPng` (synchronous CPU work) and then `resizeImage` (worker thread); both are CPU-bound, so one blocking task preserves the non-blocking contract without an extra hand-off.
- `processImage` returns an enum instead of a `{ok:true,...} | {ok:false,...}` object; the differential asserts the identical wire shape.

## Proof inventory

- Focused cases: 8.
- Differential request: `27106` (8 fixtures).
