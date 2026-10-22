# Traceability: `packages/agent/src/harness/session/jsonl/codec.ts`

Rust: `crates/pi-agent-core/src/harness/session/jsonl/codec.rs`. Tests: `session_codec.rs`,
`differential_session.rs` (codec + full file-byte comparisons).

## Q18 native error persistence

`lossless_provider_errors::provider_failure_survives_jsonl_repo_close_and_reopen` reproduces raw provider failure loss across append/drop/reopen: scalar root parsing first skipped the valid entry; native root parsing then exposed serde tagged/flattened buffering. Codec now parses JsValue once and decodes entry/record fields directly from that authority; no scalar preparse or alternate fallback. OperationError.message is JsString. Native custom message keys/payloads and operation failures are included in the durable-boundary fixture. `session_codec::native_messages_and_payload_keys_round_trip_in_durable_records` additionally proves raw originalPrompt/initialMessages, queued/deferred targets, operation errors and effectiveArgs raw keys; existing 13 codec cases retain integer/schema/key-order regression coverage. Existing scalar session identity/header metadata contracts are not widened by this bounded consumer slice.

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

## Numeric boundary differential

`differential_session_codec_numbers.rs` runs `session-codec-numbers-oracle.ts`
against the real pinned `codec.ts`, using the shared
`pi-coding-agent/tests/support/oracle.rs` source-lock check, parent watchdog and
process-group cleanup. Raw JSON lines preserve numeric spelling. Complete parse
values and exact error kind/message are compared without numeric normalization
or field filtering; Rust entry/record variants are represented with the TS API's
wrapper rather than their flattened on-disk serde representation.

| Branch | Concrete coverage | Proof |
|---|---|---|
| `requireSequence` in entry/record/lane/fact | `2^53-1`, `2^53`, negative boundaries, positive/zero/negative, `-0.0`, `1.0`, fractions and integral/fractional exponents | Pinned TS differential |
| `requireTimestamp` in header `createdAt`, entry/record `timestamp` | Same spellings; zero accepted, negatives/fractions/unsafe integers rejected | Pinned TS differential |
| First error | Unsafe seq before dispatch/id; id/parentId/lane before timestamp; timestamp before customType/intent/cwd; header kind/version/parent before createdAt | Exact schema kind/message, 12 competing-error fixtures |
| Header `version !== 4` | `4`, `4.0`, `4e0` accepted; `4.5`, `3`, string `"4"` rejected | Pinned TS differential; reproduced `4.0` failure on `432cf07` before changing version check |

Regression RED: replacing only isolated-tree `codec.rs` with `7b7d800^` made all
three differential tests fail: unsafe `createdAt` accepted, unsafe seq reported
unknown mutation kind instead of invalid seq, and `4.0` version rejected. Restoring
`432cf07` passed numeric boundaries/order but still failed version equality.
The production fix changes only version comparison to numeric equality; no new
numeric helper or safe-integer implementation was added.

## Gaps

- Typed deserialization rejects malformed nested messages/intents that TS would accept and carry through. TS-written
  session files always contain valid shapes.
- Unknown extra properties are dropped (see `session-types.md`).
