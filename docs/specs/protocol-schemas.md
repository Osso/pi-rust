# Protocol schemas

Protocol schemas port typed contracts from pinned `packages/protocol/src/schemas.ts`. Rust models each field with its schema-specific type instead of exposing generic CBOR or JavaScript value categories.

## What it must do

### Thinking levels

- [x] Represent exactly `off`, `minimal`, `low`, `medium`, `high`, `xhigh`, and `max`.
- [x] Convert each enum value back to its exact source literal.
- [x] Reject every other spelling, including the broader `pi-ai` value `ultra`.

### Session phases

- [x] Represent exactly `idle`, `turn`, `compaction`, `branch_summary`, and `retry`.
- [x] Convert each enum value back to its exact source literal and reject every other spelling.

### Model references

- [x] Own and preserve the exact `provider` and `id` strings.
- [x] Accept every Rust-representable non-empty string, including Unicode, whitespace-only, combining, and embedded-NUL values.
- [x] Reject an empty `provider` before checking `id`, then reject an empty `id`.

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

- `crates/pi-protocol/tests/schema_leaves.rs`
- `crates/pi-protocol/tests/model_cost.rs`
- `crates/pi-protocol/tests/differential_protocol/model_cost.rs`

## Known gaps (current cycle)

- [ ] Compare thinking-level, session-phase, and model-reference fixtures directly with the pinned TypeBox schemas.

## Out of scope

- Missing, wrong-typed, and additional object fields at typed constructor boundaries; future JSONL or private wire decoders enforce those constraints.
- JavaScript strings containing lone UTF-16 surrogates, which Rust `String` cannot represent.
- Remaining protocol schemas and integer fields.
- Generic `JsonValue` representation.
- CBOR encoding, decoding, and options.
- Decimal or exact-money arithmetic; source `number` fields use IEEE-754 binary64 semantics.
