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
- [x] Classify the RPC JSON serializer as semantic JSON rather than byte-identical `JSON.stringify`, approved August 12, 2026.
- [x] Classify truncation limits as typed non-negative integers and single-line truncation as Unicode-scalar-safe, approved August 12, 2026.
- [x] Preserve the shell sanitizer's explicit code-point ranges rather than applying broad Unicode control categories.
- [x] Preserve exact coding-agent auth command names, usage strings, and help selection over Rust-representable strings.
- [x] Preserve exact coding-agent auth parser errors, argument ordering, flags, and field-specific expiry-number behavior against the pinned source.
- [ ] Prove pure config-value command classification and environment-reference parsing against the pinned source.

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
- `crates/pi-protocol/tests/model_metadata.rs`
- `crates/pi-protocol/tests/schema_leaves.rs`
- `crates/pi-protocol/tests/usage.rs`
- `crates/pi-protocol/tests/content_leaves.rs`
- `crates/pi-protocol/tests/session_metadata.rs`
- `crates/pi-protocol/tests/protocol_error_code.rs`
- `crates/pi-protocol/tests/commands.rs`
- `crates/pi-protocol/tests/client_hello.rs`
- `crates/pi-protocol/tests/message_envelopes.rs`
- `crates/pi-protocol/tests/differential_protocol/commands.rs`
- `crates/pi-protocol/tests/differential_protocol/client_hello.rs`
- `crates/pi-protocol/tests/differential_protocol/message_envelopes.rs`
- `crates/pi-protocol/tests/differential_protocol/content_leaves.rs`
- `crates/pi-protocol/tests/differential_protocol/model_metadata.rs`
- `crates/pi-protocol/tests/differential_protocol/protocol_error_code.rs`
- `crates/pi-protocol/tests/differential_protocol/schema_leaves.rs`
- `crates/pi-protocol/tests/differential_protocol/session_metadata.rs`
- `crates/pi-protocol/tests/differential_protocol/usage.rs`
- `crates/pi-coding-agent/tests/rpc_jsonl.rs`
- `crates/pi-coding-agent/tests/differential_rpc_jsonl.rs`
- `crates/pi-coding-agent/tests/auth_command.rs`
- `crates/pi-coding-agent/tests/differential_auth_command.rs`
- `crates/pi-coding-agent/tests/config_value_references.rs`
- `crates/pi-agent-core/tests/truncate.rs`
- `crates/pi-agent-core/tests/differential_truncate.rs`
- `crates/pi-agent-core/tests/sanitize_binary_output.rs`
- `crates/pi-agent-core/tests/differential_truncate/sanitize.rs`

## Known gaps (current cycle)

- [ ] Classify and resolve the payload-above-4-GiB proof gap.

## Out of scope

- Invalid JavaScript runtime input types at typed Rust API boundaries, approved August 12, 2026.
- JavaScript lone-surrogate strings that Rust `String` cannot represent.
- Positive integer-valued JavaScript schema values above `u64::MAX` for typed integer fields, approved August 12, 2026.
- Malformed or manually forged oracle requests; the Rust harness emits only typed operations and valid hexadecimal bytes.
- Exact JavaScript allocation exception identity. Approved Rust behavior returns `FrameDecodeError::Allocation` and terminally fails the decoder.
- Generic public CBOR or JavaScript-number value models; typed schema fields use their actual Rust numeric types.
- Refactoring behavior while translating it.
- Treating TypeScript as the intended-behavior authority when a specification or invariant contradicts it.
- Byte-identical `JSON.stringify` output in RPC JSONL. Stock `serde_json` semantic output was approved August 12, 2026; exact number-spelling discrepancies are pinned against the source, Rust map order follows the selected serializer, and NaN/infinities are proven to match as `null`.
- Negative, fractional, NaN, and infinite JavaScript truncation limits; Rust uses non-negative integer counts.
- UTF-16 code-unit splitting and lone-surrogate output from JavaScript `truncateLine`; Rust truncates at Unicode scalar boundaries.
- Claiming full Pi compatibility before all package contracts are ported and integrated.
