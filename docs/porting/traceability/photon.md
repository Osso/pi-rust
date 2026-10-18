# coding-agent photon traceability

Pinned source: `packages/coding-agent/src/utils/photon.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Differential proof: requests `27100`-`27106` (image pipeline oracle) decode and re-encode PNG, JPEG, WebP, GIF, and BMP fixtures through both implementations and compare base64 payloads and pixel buffers exactly. Focused tests: `crates/pi-coding-agent/tests/photon.rs`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `photon.ts:17-23` | Re-export `PhotonImage` type | `photon_rs::PhotonImage` (dependency type); used by `decode_image` |
| `photon.ts:27-35` | WASM path helpers for the patched `fs.readFileSync` | Not ported: the backend is linked statically, so there is no file to locate |
| `photon.ts:37-93` | Patch/restore `fs.readFileSync` so Bun executables find the WASM module | Not ported: no runtime module load exists (see gap) |
| `photon.ts:95-121` | `loadPhoton()` returns the module or `null`, cached | `decode_image`/`png_bytes`/`jpeg_bytes` are infallible to load; `None` branches in callers collapse (see gap) |
| `@silvia-odwyer/photon-node` `new_from_byteslice` | Decode bytes to RGBA, panic on failure | `decode_image` → `PhotonError::Decode`; focused `rejects_undecodable_bytes` |
| `PhotonImage.get_bytes` | Encode raw pixels as PNG | `png_bytes`; focused `encodes_png_and_jpeg_from_raw_pixels`, differential |
| `PhotonImage.get_bytes_jpeg(quality)` | Encode raw pixels as JPEG at `quality` | `jpeg_bytes`; focused quality-sensitivity case, differential quality steps |
| `photon_rs::transform::SamplingFilter` | Filter enum passed to `resize` | `SamplingFilter` re-export, `RESIZE_FILTER = Lanczos3` |
| `PhotonImage.new` | Build an image from raw RGBA | `PhotonImage::new` (dependency), used by `rotate_quarter_turn` |

## Gaps and discrepancies

- **Gap:** `loadPhoton()` and the WASM `readFileSync` patch have no Rust equivalent. The pinned source returns `null` when the module cannot load, and every caller then returns `null`/omits the image. The Rust port links `photon-rs` directly, so the "backend unavailable" branch is unreachable. Callers therefore have one code path instead of two; no observable behavior changes when the backend is present.
- **Discrepancy:** `photon-rs` exposes only panicking decode/encode wrappers (`unwrap()`). The port performs the same `image` crate calls (`load_from_memory`, `to_rgba8`, `DynamicImage::write_to`) and returns `Result`, which reproduces the source's `catch { return null }` behavior instead of aborting the process.
- `encode` failures are typed `PhotonError::Encode` instead of an exception.

## Proof inventory

- Focused cases: 4.
- Differential: image pipeline requests `27102`-`27106` (decode/encode through convert, resize, and process paths).
