# Protocol schemas

Protocol schemas port the typed contracts from pinned `packages/protocol/src/schemas.ts`. Rust models each field with its schema-specific type rather than exposing a generic CBOR or JavaScript-number representation.

## What it must do

### Model costs

- [x] Represent `input`, `output`, `cacheRead`, and `cacheWrite` as `f64` values.
- [x] Accept every finite value greater than or equal to zero, including integers, fractions, positive zero, and negative zero.
- [x] Preserve the IEEE-754 bits of accepted values, including negative zero.
- [x] Reject negative, NaN, and infinite values.
- [x] Report the first invalid field in source schema order.

## How it works

- [Schema traceability](../porting/traceability/protocol-schemas.md)
- [Differential port system](../wiki/systems/differential-port.md)

## Implementation inventory

- `crates/pi-protocol/src/schemas.rs` — typed schema models and constructor validation.

## Tests asserting this spec

- `crates/pi-protocol/tests/model_cost.rs`
- `crates/pi-protocol/tests/differential_model_cost.rs`

## Known gaps (current cycle)

- [ ] Add direct TypeBox differential proof for `ModelCostSchema`.

## Out of scope

- Object-shape and unknown-field validation at typed constructor boundaries; future JSONL or private wire decoders enforce those constraints.
- Other protocol schemas and integer fields.
- Generic `JsonValue` representation.
- CBOR encoding, decoding, and options.
- Decimal or exact-money arithmetic; source `number` fields use IEEE-754 binary64 semantics.
