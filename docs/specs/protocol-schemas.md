# Protocol schemas

Protocol schemas port typed contracts from pinned `packages/protocol/src/schemas.ts`. Rust models each field with its schema-specific type instead of exposing generic CBOR or JavaScript value categories.

## What it must do

### Thinking levels

- [x] Represent exactly `off`, `minimal`, `low`, `medium`, `high`, `xhigh`, and `max`.
- [x] Expose `ThinkingLevel::as_str` and `FromStr` with exact source spellings.
- [x] Reject every other spelling, including the broader `pi-ai` value `ultra`.

### Session phases

- [x] Represent exactly `idle`, `turn`, `compaction`, `branch_summary`, and `retry`.
- [x] Expose `SessionPhase::as_str` and `FromStr` with exact source spellings and reject every other spelling.

### Model references

- [x] Construct owned values through `ModelRef::new` and preserve exact strings through `provider` and `id` getters.
- [x] Accept every Rust-representable non-empty string, including Unicode, whitespace-only, combining, and embedded-NUL values.
- [x] Return `ModelRefError::EmptyProvider` first when both fields are empty, then `EmptyId` for an empty identifier.

### Model costs

- [x] Represent `input`, `output`, `cacheRead`, and `cacheWrite` as `f64` values.
- [x] Accept every finite value greater than or equal to zero, including integers, fractions, positive zero, and negative zero.
- [x] Preserve the IEEE-754 bits of accepted values, including negative zero.
- [x] Reject negative, NaN, and infinite values.
- [x] Report the first invalid field in source schema order.

### Model metadata

- [x] Represent `input` through exact `text` and `image` literals, preserving array order and duplicates and allowing an empty array.
- [x] Own and preserve non-empty `provider`, `id`, `name`, and `api` strings without trimming or normalization.
- [x] Preserve `reasoning`, `authenticated`, validated `ModelCost`, and a non-empty ordered list of `ThinkingLevel` values.
- [x] Represent positive `contextWindow` and `maxTokens` values as `u64`, including `u64::MAX`.
- [x] Report constructor validation errors in representable source field order.

## How it works

- [Schema traceability](../porting/traceability/protocol-schemas.md)
- [Differential port system](../wiki/systems/differential-port.md)

## Implementation inventory

- `crates/pi-protocol/src/lib.rs` — exports schema types and validation errors.
- `crates/pi-protocol/src/schemas.rs` — schema-specific enums, models, parsing, and constructor validation.

## Tests asserting this spec

- `crates/pi-protocol/tests/schema_leaves.rs`
- `crates/pi-protocol/tests/model_cost.rs`
- `crates/pi-protocol/tests/model_metadata.rs`
- `crates/pi-protocol/tests/differential_protocol/model_cost.rs`
- `crates/pi-protocol/tests/differential_protocol/schema_leaves.rs`

## Known gaps (current cycle)

- [ ] Compare `ModelMetadataSchema` directly with the pinned TypeBox schema.

## Out of scope

- Missing, wrong-typed, and additional object fields at typed constructor boundaries; future JSONL or private wire decoders enforce those constraints.
- JavaScript strings containing lone UTF-16 surrogates, which Rust `String` cannot represent.
- Positive integer-valued JavaScript numbers above `u64::MAX`; Alessio approved `u64` typed fields on August 12, 2026.
- Remaining protocol schemas and integer fields.
- Generic `JsonValue` representation.
- CBOR encoding, decoding, and options.
- Decimal or exact-money arithmetic; source `number` fields use IEEE-754 binary64 semantics.
