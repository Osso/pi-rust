# Protocol framing

Protocol framing ports `packages/protocol/src/framing.ts` from the pinned TypeScript source. This cycle covers frame construction and complete-frame validation, not incremental decoding.

## What it must do

### Construction

- [ ] Prefix every payload with its unsigned 32-bit big-endian byte length.
- [ ] Return newly owned bytes so later input mutation cannot change the frame.
- [ ] Encode an empty payload as four zero bytes.
- [ ] Reject payload lengths above the unsigned 32-bit limit.

### Validation

- [ ] Accept exactly one complete frame at or below the configured maximum.
- [ ] Use 16 MiB as the default maximum payload length.
- [ ] Reject incomplete length prefixes.
- [ ] Reject declared lengths above the configured maximum.
- [ ] Reject partial payloads and trailing bytes with the source error messages.

## How it works

- [Differential port system](../wiki/systems/differential-port.md)
- [Protocol-framing traceability](../porting/traceability/protocol-framing.md)

## Implementation inventory

- `crates/pi-protocol/src/framing.rs` — typed Rust frame construction and validation.
- `oracle/framing-oracle.ts` — adapter over the real TypeScript functions.

## Tests asserting this spec

- `crates/pi-protocol/tests/framing.rs`
- `crates/pi-protocol/tests/differential_framing.rs`

## Known gaps (current cycle)

- [ ] Implement and prove frame construction.
- [ ] Implement and prove complete-frame validation.

## Out of scope

- `FrameDecoder`; it is a later stateful slice.
- Runtime rejection of non-`Uint8Array` values; typed Rust byte-slice APIs make those inputs unrepresentable by approved decision.
- Runtime rejection of fractional, non-finite, negative, or over-`u32` maximum values; Rust accepts `Option<u32>`.
- Allocating a payload above 4 GiB solely to exercise the source overflow guard.
