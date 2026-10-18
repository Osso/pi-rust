# Traceability: `packages/agent/src/harness/session/jsonl/codec.ts`

Rust: `crates/pi-agent-core/src/harness/session/jsonl/codec.rs`. Tests: `session_codec.rs`,
`differential_session.rs` (codec + full file-byte comparisons).

## Exports and branches

| TypeScript | Rust | Tests | Notes |
|---|---|---|---|
| `parseHeader` | `parse_header` | codec, differential | Validates `kind`, version 4, id/cwd strings, non-negative timestamp, parent/legacy exclusivity, object metadata. |
| `encodeHeader` | `encode_header` | codec, differential | Exact key order and trailing LF. |
| `metadataFromHeader` | `metadata_from_header` | codec, differential | `sourceFormat: 4`, optional fields omitted. |
| `parseMutation` | `parse_mutation` | codec, differential | Dispatches on `kind`; validates seq/type/lane/id/timestamp/customType/intent kind/runId. |
| `encodeMutation` | `encode_mutation` | codec, differential | `kind` first; entry emits `lane` only when present; trailing LF. |
| `ENTRY_TYPES`/`RECORD_TYPES`/`OPERATION_KINDS` sets | `known_entry_type`, `known_record_type`, inline kind match | codec | Unknown discriminants produce the TS messages. |

## Byte contract

- Canonical field order matches records produced by the TS `Session` facade: entries `type, id, <fields>, parentId,
  seq, timestamp`; records `type, id, lane, <fields>, seq, timestamp`.
- TS `encodeMutation` preserves caller object insertion order. Rust has one canonical order. Files written by the
  TS API and by Rust are therefore byte-identical; an arbitrary caller-built mutation object with a different key
  order would encode differently in TS. This is documented as a caller-order caveat, not a session-file contract
  difference.
- Numbers use `pi_js_compat::number` (`f64` → JavaScript formatting), timestamps and sequences are `i64`.
- `OptionalValue` keeps explicit `null` for `data`/`details` while omitting absent properties.

## Gaps

- Typed deserialization rejects malformed nested messages/intents that TS would accept and carry through. TS-written
  session files always contain valid shapes.
- Unknown extra properties are dropped (see `session-types.md`).
