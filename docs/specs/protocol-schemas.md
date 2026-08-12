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
- [x] Use Rust-specific constructor diagnostics ordered by the source field declarations.

### Usage

- [x] Represent non-negative token counts as `u64`, preserving provider values without recomputing `totalTokens`.
- [x] Preserve omitted reasoning separately from explicit zero through `Option<u64>`.
- [x] Represent all five usage-cost fields as finite non-negative `f64` values and preserve accepted IEEE-754 bits.
- [x] Use Rust-specific usage-cost diagnostics ordered by the source field declarations.

### Content leaves

- [x] Preserve every text and thinking string, including empty, Unicode, and embedded-NUL values.
- [x] Preserve omitted, false, and true thinking-redaction states through `Option<bool>`.
- [x] Preserve image data as an opaque string without base64 validation or normalization.
- [x] Require only a non-empty image MIME type; whitespace-only values remain valid.
- [x] Represent exact content tags through distinct Rust types without redundant stored tag fields.

### Session metadata

- [x] Preserve non-empty session IDs and non-negative creation timestamps.
- [x] Map omitted update timestamps, parent IDs, names, and working directories to `None` without defaults.
- [x] Distinguish omitted session names from explicit empty names; `null` remains outside the typed constructor.
- [x] Require optional parent IDs and working directories to be non-empty when present.
- [x] Represent timestamps as `u64`; focused proof reaches `u64::MAX`, while direct TypeBox proof reaches `Number.MAX_SAFE_INTEGER`.

### Protocol error codes

- [x] Represent exactly `version`, `busy`, `session_locked`, `not_found`, `invalid_request`, `not_implemented`, and `internal_error`.
- [x] Expose exact `ProtocolErrorCode::as_str` and `FromStr` mappings without aliases or normalization.

### Commands

- [x] Represent all nine command leaves as validated private-field Rust types and compose them through `Command`.
- [x] Preserve exact command literals through `Command::as_str`.
- [x] Preserve omitted create options, unrestricted optional names, unrestricted prompt/steer text, and validated nested model/thinking values.
- [x] Require create working directories and all required session IDs to be non-empty.

### Client hello

- [x] Expose protocol version `1` through `PROTOCOL_VERSION`.
- [x] Construct typed `ClientHello` values from any Rust `u64` without a caller-supplied tag.
- [x] Preserve every non-negative Rust version, including zero and `u64::MAX`.
- [x] Keep schema acceptance separate from supported-version negotiation by accepting versions other than `PROTOCOL_VERSION`.

## How it works

- [Schema traceability](../porting/traceability/protocol-schemas.md)
- [Differential port system](../wiki/systems/differential-port.md)

## Implementation inventory

- `crates/pi-protocol/src/lib.rs` — exports schema types and validation errors.
- `crates/pi-protocol/src/schemas.rs` — schema APIs and public re-exports.
- `crates/pi-protocol/src/schemas/session_metadata.rs` — `SessionMetadataInput`, `SessionMetadata::new`, six getters, and `SessionMetadataError::{EmptyId, EmptyParentSessionId, EmptyCwd}`.
- `crates/pi-protocol/src/schemas/protocol_error_code.rs` — `ProtocolErrorCode`, `as_str`, `FromStr`, and `ParseProtocolErrorCodeError`.
- `crates/pi-protocol/src/schemas/commands.rs` — command leaves, constructors/getters, `Command`, and `CommandError`.
- `crates/pi-protocol/src/schemas/client_hello.rs` — protocol version and typed client handshake.

## Tests asserting this spec

- `crates/pi-protocol/tests/schema_leaves.rs`
- `crates/pi-protocol/tests/model_cost.rs`
- `crates/pi-protocol/tests/model_metadata.rs`
- `crates/pi-protocol/tests/usage.rs`
- `crates/pi-protocol/tests/content_leaves.rs`
- `crates/pi-protocol/tests/session_metadata.rs`
- `crates/pi-protocol/tests/protocol_error_code.rs`
- `crates/pi-protocol/tests/commands.rs`
- `crates/pi-protocol/tests/client_hello.rs`
- `crates/pi-protocol/tests/differential_protocol/commands.rs`
- `crates/pi-protocol/tests/differential_protocol/client_hello.rs`
- `crates/pi-protocol/tests/differential_protocol/content_leaves.rs`
- `crates/pi-protocol/tests/differential_protocol/model_cost.rs`
- `crates/pi-protocol/tests/differential_protocol/model_metadata.rs`
- `crates/pi-protocol/tests/differential_protocol/protocol_error_code.rs`
- `crates/pi-protocol/tests/differential_protocol/schema_leaves.rs`
- `crates/pi-protocol/tests/differential_protocol/session_metadata.rs`
- `crates/pi-protocol/tests/differential_protocol/usage.rs`

## Known gaps (current cycle)

- [ ] Port `RequestEnvelopeSchema` and compose `ClientMessageSchema`, including exact wire-tag validation.

## Out of scope

- Parsing source tag properties plus missing, wrong-typed, and additional object fields; future JSONL or private wire decoders enforce those constraints.
- JavaScript strings containing lone UTF-16 surrogates, which Rust `String` cannot represent.
- Direct integer differential proof above JavaScript `Number.MAX_SAFE_INTEGER`; larger `u64` values have Rust-side focused proof, while positive TypeBox integers above `u64::MAX` are excluded. Alessio approved `u64` typed fields on August 12, 2026.
- Full `ProtocolErrorSchema` while optional `details` depends on the excluded generic `JsonValue` contract.
- `SessionSnapshotSchema` and snapshot result schemas until transcript dependencies are available.
- `ToolCallContentSchema`, content unions, and transcript schemas because the generic `JsonValue` contract remains excluded.
- CBOR encoding, decoding, and options.
- Decimal or exact-money arithmetic; source `number` fields use IEEE-754 binary64 semantics.
