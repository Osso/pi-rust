# Protocol framing

Protocol framing ports `packages/protocol/src/framing.ts` from the pinned TypeScript source. It covers frame construction, complete-frame validation, and incremental decoding of arbitrary byte chunks.

## What it must do

### Construction

- [x] Prefix every payload with its unsigned 32-bit big-endian byte length.
- [x] Return newly owned bytes so later input mutation cannot change the frame.
- [x] Encode an empty payload as four zero bytes.
- [ ] Reject payload lengths above the unsigned 32-bit limit.

### Validation

- [x] Accept exactly one complete frame at or below the configured maximum.
- [x] Use 16 MiB as the default maximum payload length.
- [x] Reject incomplete length prefixes.
- [x] Reject declared lengths above the configured maximum.
- [x] Reject partial payloads and trailing bytes with the source error messages.

### Incremental decoding

- [x] Apply the default or configured unsigned 32-bit maximum when constructing a decoder.
- [x] Retain fragmented headers and payloads across pushes.
- [x] Decode coalesced, fragmented, and empty frames in wire order.
- [x] Preserve payloads across the 64 KiB internal block boundary.
- [x] Copy input bytes so returned payloads do not alias chunks or later decoder activity.
- [x] Reject oversized headers immediately, discard earlier frames from the same failed push, and enter terminal failed state.
- [x] Accept a clean end and reject truncated headers or payloads with the source message.
- [x] Keep ended and failed states terminal with the source messages.
- [x] Return `FrameDecodeError::Allocation`, clear partial buffers, and enter failed state when Rust cannot reserve decoder-owned memory.

## How it works

- [Differential port system](../wiki/systems/differential-port.md)
- [Protocol-framing traceability](../porting/traceability/protocol-framing.md)

## Implementation inventory

- `crates/pi-protocol/src/framing.rs` — typed frame construction, validation, and incremental decoding.
- `oracle/framing-oracle.ts` — adapter over the real TypeScript framing functions.

## Tests asserting this spec

- `crates/pi-protocol/src/framing/tests.rs`
- `crates/pi-protocol/tests/framing.rs`
- `crates/pi-protocol/tests/frame_decoder.rs`
- `crates/pi-protocol/tests/differential_framing.rs`

## Known gaps (current cycle)

- [ ] Add ordered TypeScript differential proof for `FrameDecoder`.
- [ ] Resolve proof for the impractical payload-above-4-GiB construction guard.

## Out of scope

- Runtime rejection of non-`Uint8Array` values; typed Rust byte-slice APIs make those inputs unrepresentable by approved decision.
- Runtime rejection of fractional, non-finite, negative, or over-`u32` maximum values; Rust accepts `Option<u32>`.
- Exact JavaScript allocation exception identity; Rust uses the approved typed allocation error and terminal failed-state policy.
- Allocating a payload above 4 GiB solely to exercise the construction overflow guard.
- 32-bit targets; all intended Pi binary targets are 64-bit.
