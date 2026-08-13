# Agent-core image-helper traceability

Pinned source: `packages/agent/src/harness/tools/image.ts:1-104` at `532f7d12cd19235081bd4d6e425bde185652d86f`. This slice ports internal byte helpers only. It does not alter protocol `ImageContent`, image decoding, or file operations.

| TypeScript source | Observable behavior | Rust proof or gap |
|---|---|---|
| `image.ts:1-10` | Apply JPEG, PNG, GIF, WebP, BMP, then unknown precedence and exact MIME literals | `detect_supported_image_mime_type`; focused signature/near-miss, PNG, and BMP tests in `crates/pi-agent-core/tests/image.rs`; direct differential proof pending (`13069..=13080`) |
| `image.ts:4` | Accept `ff d8 ff`, accept exactly three bytes, reject when byte index 3 is exactly `f7` | Focused `detects_short_signatures_and_case_sensitive_near_misses` |
| `image.ts:5,27-45` | Require PNG signature, length/`IHDR` structure, preserve minimal 16-byte acceptance, detect `acTL` before `IDAT`, stop static at `IDAT`, and return static for truncated/oversized unrelated chunks | Focused `accepts_minimal_png_and_preserves_apng_chunk_order_quirks` and `rejects_png_near_misses_before_chunk_scanning` |
| `image.ts:6-7` | Accept exact case-sensitive GIF prefix and RIFF/WEBP offsets without deeper validation | Focused `detects_short_signatures_and_case_sensitive_near_misses` |
| `image.ts:8,47-69` | Preserve BMP size, offset, DIB, planes, and accepted bits-per-pixel branches, including size-zero and short modern-header quirks | Focused `accepts_bmp_core_and_modern_boundary_variants` and `rejects_bmp_invalid_size_offset_header_planes_and_depth` |
| `image.ts:12-25` | Encode standard Base64 with exact alphabet, groups, padding, empty behavior, and no wrapping | Focused `encodes_empty_and_partial_base64_groups_with_padding` and `encodes_high_bit_alphabet_boundaries_and_multiple_groups` |
| `image.ts:71-104` | Read little/big-endian fields and perform exact case-sensitive prefix checks without throwing on missing bytes | Covered through MIME branch fixtures; direct differential proof pending |

## Rust boundary and discrepancies

- Public functions are available only through `pi_agent_core::harness::tools::image`; neither helper is re-exported from `harness` or `harness::tools`, matching the pinned module/package export boundary.
- Rust uses `&[u8]` and static MIME strings. Missing JavaScript byte reads map to zero in internal readers; checked Rust offset arithmetic returns the source-compatible non-detected/static result rather than panicking.
- The focused slice intentionally excludes decoding, validation beyond the pinned checks, file I/O, data URLs, compression, protocol `ImageContent`, and generic binary values.

## Proof status

- Focused RED: `/tmp/pi-rust-image-helpers-red.out` records the missing `harness::tools` module/API.
- Focused GREEN: `/tmp/pi-rust-image-helpers-green.out` records 7 passing tests and 0 failures.
- Direct pinned differential proof remains a known gap; reserved request IDs are `13069..=13080`.
