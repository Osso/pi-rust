# RPC JSONL framing

RPC JSONL framing ports `packages/coding-agent/src/modes/rpc/jsonl.ts` from the pinned TypeScript source. It covers compact JSON record serialization and transport-neutral incremental decoding of LF-delimited UTF-8 chunks.

## What it must do

### Serialization

- [x] Serialize Rust `Serialize` values as compact JSON followed by exactly one LF.
- [x] Preserve U+2028 and U+2029 as payload characters rather than record separators.
- [x] Escape LF characters inside JSON strings so they do not create records.
- [x] Propagate `serde_json` serialization errors.
- [x] Use the approved semantic-JSON policy rather than claiming byte-identical `JSON.stringify` number spelling.

### Incremental decoding

- [x] Accept arbitrary byte chunks and buffer incomplete UTF-8 sequences across pushes.
- [x] Match Node `StringDecoder("utf8")` replacement behavior for malformed bytes and incomplete final sequences.
- [x] Preserve a UTF-8 BOM and treat empty chunks as no-ops.
- [x] Accept already-decoded string chunks without consuming pending byte-decoder state.
- [x] Split records only on LF and emit every complete record in arrival order.
- [x] Strip exactly one trailing CR from each emitted record while preserving bare or internal CR characters.
- [x] Emit consecutive empty records, but do not invent an additional record after a terminal LF.
- [x] Emit one non-empty final unterminated record on end and emit nothing for an empty final buffer.

## How it works

- [RPC JSONL traceability](../porting/traceability/rpc-jsonl.md)

## Implementation inventory

- `crates/pi-coding-agent/src/modes/rpc/jsonl.rs` — compact JSON serialization and incremental line decoder.
- `crates/pi-coding-agent/src/modes/rpc/mod.rs` — public RPC-mode exports.
- `crates/pi-coding-agent/tests/rpc_jsonl.rs` — focused serialization and decoder behavior.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/rpc_jsonl.rs`

## Known gaps (current cycle)

- [ ] Compare shared serialization and decoder sequences directly with the pinned TypeScript source.
- [ ] Record the approved number-spelling discrepancies through pinned source probes.

## Out of scope

- JSON parsing, RPC command or response schemas, dispatch, and error responses.
- stdin/stdout ownership, Node stream listener attachment or detachment, backpressure, stream errors, and callback ordering with unrelated listeners.
- JavaScript-only serialization inputs such as `undefined`, functions, symbols, `BigInt`, cyclic objects, and throwing `toJSON` methods.
- Byte-identical `JSON.stringify` number spelling. Alessio approved stock `serde_json` semantic JSON on August 12, 2026; known differences include integral floats, negative zero, exponent thresholds, and Rust integers without exact JavaScript Number equivalents.
- Line-length limits or malformed-JSON validation.
