# coding-agent image-resize-worker traceability

Pinned source: `packages/coding-agent/src/utils/image-resize-worker.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

The module is a `worker_threads` entry point with no exports: it receives one message, calls `resizeImageInProcess`, and posts `{result}` or `{error}`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `image-resize-worker.ts:5-10` | Request shape (`inputBytes`, `mimeType`, `options`) | The `spawn_blocking` closure in `utils::image_resize::resize_image` carries the same three inputs |
| `image-resize-worker.ts:12-16` | Request guard; invalid message → error response | Not ported: the closure is typed, so invalid requests are unrepresentable |
| `image-resize-worker.ts:18-21` | Worker requires `parentPort` | Not ported: no worker boundary |
| `image-resize-worker.ts:23-40` | Run the resize, post `{result}` or `{error: message}` | `resize_image` returns the result directly; panics are resumed on the caller |

## Gaps

- **Gap:** there is no worker message protocol in Rust. The offloading contract (do not block the event loop) is provided by `tokio::task::spawn_blocking`; the response envelope, guard, and `parentPort` requirement have no counterpart. Behavioral proof that the offload returns the same bytes as the in-process path is the differential `resize_in_worker` request `27104`, which runs the source's real `resizeImage` (worker attempt with in-process fallback under `tsx`).
