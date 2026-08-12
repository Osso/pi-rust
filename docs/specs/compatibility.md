# TypeScript compatibility

The Rust port uses the pinned Pi TypeScript worktree as a behavioral oracle. Implementation structure may differ, but every representable observable result must match unless Alessio approves a documented deviation.

## What it must do

- [x] Refuse differential proof when the source worktree is not at the pinned commit.
- [x] Execute the real pinned TypeScript functions through the worktree-local `tsx` runtime.
- [x] Send identical fixtures to TypeScript and Rust and compare normalized values and errors.
- [x] Execute ordered `push` and `end` steps against one TypeScript and Rust decoder, comparing every success and error without replacing the instance.
- [x] Map each ported TypeScript statement and branch to behavioral proof or an explicit gap.
- [x] Represent protocol numbers with schema-specific Rust types instead of preserving JavaScript or CBOR numeric categories.
- [ ] Classify discrepancies as a Rust bug, TypeScript bug, approved deviation, or underspecified behavior before changing semantics.
- [x] Stop and ask Alessio when Rust cannot reproduce TypeScript behavior within the approved typed-core boundary.

## How it works

- [Differential port system](../wiki/systems/differential-port.md)

## Implementation inventory

- `docs/porting/source-lock.json` — pins the read-only TypeScript oracle.
- `oracle/` — invokes selected TypeScript source functions.
- `crates/*/tests/differential_*.rs` — compares Rust results with the oracle.
- `docs/porting/traceability/` — maps pinned source statements to proof.

## Tests asserting this spec

- `crates/pi-protocol/tests/differential_protocol.rs`
- `crates/pi-protocol/tests/model_cost.rs`
- `crates/pi-protocol/tests/schema_leaves.rs`
- `crates/pi-protocol/tests/differential_protocol/schema_leaves.rs`

## Known gaps (current cycle)

- [ ] Classify and resolve the payload-above-4-GiB proof gap.

## Out of scope

- Invalid JavaScript runtime input types at typed Rust API boundaries, approved August 12, 2026.
- JavaScript lone-surrogate strings that Rust `String` cannot represent.
- Positive integer-valued JavaScript schema values above `u64::MAX` for typed token-count fields, approved August 12, 2026.
- Malformed or manually forged oracle requests; the Rust harness emits only typed operations and valid hexadecimal bytes.
- Exact JavaScript allocation exception identity. Approved Rust behavior returns `FrameDecodeError::Allocation` and terminally fails the decoder.
- Generic public CBOR or JavaScript-number value models; typed schema fields use their actual Rust numeric types.
- Refactoring behavior while translating it.
- Treating TypeScript as the intended-behavior authority when a specification or invariant contradicts it.
- Claiming full Pi compatibility before all package contracts are ported and integrated.
