# coding-agent image-resize traceability

Pinned source: `packages/coding-agent/src/utils/image-resize.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Differential proof: request `27104` runs the public `resizeImage` entry point on the same 15 fixtures as the in-process request and compares the records byte-for-byte. Focused tests: `crates/pi-coding-agent/tests/image_resize.rs`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `image-resize.ts:5` | Re-export `ImageResizeOptions`, `ResizedImage` | `image_resize_core::{ImageResizeOptions, ResizedImage}` |
| `image-resize.ts:13-17` | Copy input bytes before handing them to the worker | `resize_image` clones the slice into the blocking task; focused `keeps_caller_input_bytes_intact` |
| `image-resize.ts:19-21` | Worker response guard | Not ported: no message boundary (see gap) |
| `image-resize.ts:23-25` | `createResizeWorker` | `tokio::task::spawn_blocking` (see `image-resize-worker` doc) |
| `image-resize.ts:27-76` | Worker settle/fail/exit protocol | Not ported: `JoinError` panics are re-raised on the caller |
| `image-resize.ts:85-113` | Runtime worker specifier, Bun string path, fallback to in-process | `spawn_blocking` only; no load-failure branch (see gap) |
| `image-resize.ts:119-128` | `formatDimensionNote`: `undefined` when not resized, otherwise `[Image: original WxH, displayed at wxh. Multiply coordinates by <scale> to map to original image.]` | `format_dimension_note`; focused note cases, differential `27105` |
| `scale.toFixed(2)` | ECMAScript rounding: nearest hundredth, ties to the larger value | `scale_factor` (integer arithmetic); focused `rounds_dimension_scale_like_javascript_to_fixed` (9/8 → `1.13`), differential `format_dimension_note` |

## Gaps and discrepancies

- **Gap:** `worker_threads` offloading has no Rust counterpart; CPU work runs on `spawn_blocking`, so the event loop stays free. The source's two failure paths (worker cannot load → in-process fallback; worker exits/errors → throw) do not exist, because the backend is linked and the task cannot fail to start. A panic inside the blocking task is resumed on the caller rather than converted into a fallback, which is the closest mapping of "worker failed".
- **Discrepancy:** `toFixed(2)` is not Rust's `{:.2}` (half-to-even). `scale_factor` computes hundredths with exact integer arithmetic, matching the source for every `originalWidth/width` ratio.
- `resize_image` returns `Result<Option<ResizedImage>, JoinError>`; `Ok(None)` is the source's `null`.

## Proof inventory

- Focused cases: 13 (shared with image-resize-core).
- Differential request: `27104` (15 fixtures), plus `27105` for dimension notes.
