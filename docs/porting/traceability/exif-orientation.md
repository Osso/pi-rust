# coding-agent exif-orientation traceability

Frozen source: `packages/coding-agent/src/utils/exif-orientation.ts` at `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`; whole 186-line module compared to `pi-coding-agent-base/src/utils/exif_orientation.rs`.

| Symbols / branches | Proof |
| --- | --- |
| `readOrientationFromTiff`: initial/IFD/entry bounds, LE/BE, unsigned read32, tag search, valid 1–8/default | `suite_core_2::exif_orientation` LE/BE, invalid tag values, truncated/invalid-offset fixtures; frozen pixel differential |
| `findJpegTiffOffset`: fill bytes, APP1 header/length, ordinary segment advance/bounds, missing/invalid marker | Native fill/empty/truncated fixtures and actual-source JPEG differential |
| `findWebpTiffOffset`: RIFF chunks, unsigned size, even padding, next-offset bounds before EXIF, optional Exif header | Native empty/padded/truncated/unsigned-size fixtures; actual-source WebP differential |
| `hasExifHeader`, `getExifOrientation`: JPEG/WebP dispatch, absent header/default 1 | Native nonimage/truncated/no-EXIF fixtures and differential |
| `rotate90`, `applyExifOrientation`: all eight orientations, swapped dimensions, complete RGBA permutations | `suite_core_2::differential_image_pipeline::exif_orientation_matches_common_pin`: 25 identical-byte fixtures (8 LE JPEG +8 BE JPEG +8 WebP +no EXIF); 16 native tests, including nonsquare dimension swaps |

The current pin already contains unsigned TIFF/chunk reads and bounds checks. The old trace's claimed discrepancy against 532f7d12 is retired, not carried forward. Comments now identify the actual frozen behavior.

Rust takes/returns owned images rather than preserving JavaScript identity; common owned-return approval applies. Native out-of-range byte access matches bitwise zero coercion. Pathological input/numeric extremes remain outside the current parity bar.

Native proof: `/tmp/settings-models-exif-native.log` (df894c797, parser unchanged by the following comment/test partition commit). Frozen differential rerun and targeted clippy recorded in `/tmp/settings-models-proof-ledger.md`.

Frozen differential GREEN: `/tmp/settings-models-exif-differential.log`, 02e7c6033. Targeted warning-mode clippy passes with no changed-line warnings; changed comments/test partition reviewed for readability.
