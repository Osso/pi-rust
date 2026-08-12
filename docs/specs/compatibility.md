# TypeScript compatibility

The Rust port uses the pinned Pi TypeScript worktree as a behavioral oracle. Implementation structure may differ, but every representable observable result must match unless Alessio approves a documented deviation.

## What it must do

- [x] Refuse differential proof when the source worktree is not at the pinned commit.
- [x] Execute the real pinned TypeScript functions through the worktree-local `tsx` runtime.
- [x] Send identical fixtures to TypeScript and Rust and compare normalized values and errors.
- [x] Map each ported TypeScript statement and branch to behavioral proof or an explicit gap.
- [ ] Classify discrepancies as a Rust bug, TypeScript bug, approved deviation, or underspecified behavior before changing semantics.
- [ ] Stop and ask Alessio when Rust cannot reproduce TypeScript behavior within the approved typed-core boundary.

## How it works

- [Differential port system](../wiki/systems/differential-port.md)

## Implementation inventory

- `docs/porting/source-lock.json` — pins the read-only TypeScript oracle.
- `oracle/` — invokes selected TypeScript source functions.
- `crates/*/tests/differential_*.rs` — compares Rust results with the oracle.
- `docs/porting/traceability/` — maps pinned source statements to proof.

## Tests asserting this spec

- `crates/pi-protocol/tests/differential_framing.rs`

## Known gaps (current cycle)

- [ ] Classify and resolve the payload-above-4-GiB proof gap.

## Out of scope

- Invalid JavaScript runtime input types at typed Rust API boundaries, approved August 12, 2026.
- Malformed or manually forged oracle requests; the Rust harness emits only typed operations and valid hexadecimal bytes.
- Refactoring behavior while translating it.
- Treating TypeScript as the intended-behavior authority when a specification or invariant contradicts it.
- Claiming full Pi compatibility before all package contracts are ported and integrated.
