# Agent-core image helpers

Agent-core exposes two pure internal helpers ported from `packages/agent/src/harness/tools/image.ts`: source-compatible MIME recognition and standard Base64 encoding. This contract covers those byte helpers only; it does not change the protocol `ImageContent` contract. See the [image-helper traceability](../porting/traceability/image-helpers.md) for source mappings.

## What it must do

### MIME detection

- [x] Return the exact supported MIME literals for recognized JPEG, PNG, GIF, WebP, and BMP byte patterns, with JPEG precedence before other formats.
- [x] Preserve short-prefix behavior: exactly three-byte JPEG/GIF prefixes and a 12-byte RIFF/WEBP signature can be accepted; unknown and case-changed prefixes return no MIME.
- [x] Preserve PNG structural checks, minimal 16-byte acceptance, APNG `acTL`/`IDAT` ordering, and truncated or oversized chunk behavior.
- [x] Preserve BMP declared-size, pixel-offset, DIB-size, color-plane, and bits-per-pixel branches, including the accepted 30-byte modern-header boundary and size-zero quirks.
- [x] Avoid panics and arithmetic overflow while returning the source-compatible non-detected result for out-of-buffer scans.

### Base64 encoding

- [x] Encode empty and arbitrary byte slices with the standard `A-Z a-z 0-9 + /` alphabet, three-byte groups, and `=` padding.
- [x] Preserve high-bit bytes, alphabet boundaries, multi-group output, and exact output lengths without whitespace, line wrapping, or URL-safe substitutions.

### API boundary

- [x] Expose only `pi_agent_core::harness::tools::image::{detect_supported_image_mime_type, encode_base64}` with no harness-root or tools-root re-exports.
- [x] Keep the helpers pure, standard-library-only, and independent from image decoding, file I/O, and protocol message construction.

### Differential proof

- [x] Compare MIME detection and Base64 output directly with the pinned TypeScript helpers through exactly 12 persistent differential fixtures (`13069..=13080`).

## How it works

- [Image-helper implementation](../wiki/systems/image-helpers.md)

## Implementation inventory

- `crates/pi-agent-core/src/harness/tools/image.rs` — MIME recognition and Base64 helpers.
- `crates/pi-agent-core/src/harness/tools/mod.rs` — module-only public boundary.
- `crates/pi-agent-core/src/harness/mod.rs` — public harness module registration.
- `crates/pi-agent-core/tests/image.rs` — focused MIME and Base64 behavior proof.
- `crates/pi-agent-core/tests/differential_truncate/image.rs` — 12 direct pinned differential fixture groups (`13069..=13080`).
- `oracle/truncate-oracle.ts` — persistent `image_helpers` operation and direct image-module import.
- `run-tests.sh` — focused and oracle test registration.

## Tests asserting this spec

- `crates/pi-agent-core/tests/image.rs`
- `crates/pi-agent-core/tests/differential_truncate/image.rs`

## Known gaps (current cycle)

- [ ] Run final workspace, check, Clippy, and inherited verification gates after the differential proof.

## Out of scope

- Image decoding, resizing, metadata extraction, file I/O, filesystem integration, and provider/model integration.
- Protocol `ImageContent` validation or serialization; that contract remains in the protocol schema specs.
- Data URLs, compression, generic binary models, fallback detection, and new dependencies.
- Malformed JavaScript runtime values and lone-surrogate strings, which are outside the typed Rust byte-slice boundary.
