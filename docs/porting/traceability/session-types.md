# Traceability: `packages/agent/src/harness/session/types.ts`

Rust: `crates/pi-agent-core/src/harness/session/types.rs`. Tests: `crates/pi-agent-core/tests/session_codec.rs`,
`session_memory.rs`, `session_jsonl.rs`, `differential_session.rs`.

## Exports

| TypeScript export | Rust item | Tests | Notes |
|---|---|---|---|
| `JsonValue` | `JsonValue = serde_json::Value` | codec differential | |
| `SessionStopReason` | `SessionStopReason` | session_jsonl records | `pending` excluded, `deferred` added, exact camelCase literals. |
| `IdGenerator` | `IdGenerator` trait + `UuidV7IdGenerator` | session_memory | `pi_ai::utils::uuid::uuidv7`. |
| `EntryBase` | fields on `Entry` | session_codec | `type` discriminant first, `parentId` last before storage fields. |
| `MessageEntry` … `CustomEntry` | `EntryBody` variants | session_codec, session_jsonl | Internally tagged enum flattened into `Entry`. |
| `Entry` | `Entry { body, parent_id, seq, timestamp }` | session_codec, differential | `#[serde(flatten)]` reproduces TS key order. |
| `ProvisionedEntry` | `EntryBody` | session_memory | Type alias by shape: body omits `parentId`/`seq`/`timestamp`. |
| `RecordBase` | fields on `LaneRecord` | session_codec | |
| `OperationStartedRecord` | `RecordBody::OperationStarted` | session_jsonl, differential | `intent` is `OperationIntent`, tagged by `kind`. |
| `AbortRequestedRecord` … `UsageRecord` | `RecordBody` variants | session_codec, session_jsonl | Optional fields skip serialization when absent. |
| `LaneRecord` | `LaneRecord { body, seq, timestamp }` | session_codec, differential | |
| `NewRecord` | `RecordBody` | session_memory | Body omits `seq`/`timestamp`. |
| `EntryOrder` | `EntryOrder` | session_memory, differential | `None` means `newestFirst`. |
| `EntryCursor` | `EntryCursor` | session_memory | |
| `EntryQuery` | `EntryQuery` | session_memory, differential | `limit: Option<usize>`; `Some(0)` rejected. |
| `BranchBounds` | `BranchBounds` | session_memory, differential | |
| `RecordQuery` | `RecordQuery` | session_memory, differential | |
| `SessionMetadata` | `SessionMetadata` + `SessionMetadataValue` trait | session_memory, differential | Trait carries `id()` for generic storage. |
| `SessionStats` | `SessionStats` | session_jsonl, differential | Token/cost fields `f64` with JS number serialization. |
| `LanePointer` | `LanePointer` | session_memory, differential | |
| `LogItem` | `LogItem` | session_memory, differential | `fact` items flatten `FactItem` after `seq`. |
| `LogOptions` | `LogOptions` | session_jsonl, differential | |
| `SessionStorage` | `SessionStorage<M>` trait | memory/jsonl implementations | Async methods return `BoxFuture`; Rust `Result` replaces throws. |
| `SessionTree` | `Session`/`SessionView` methods | session_memory | Views are lane-bound facades. |
| `SessionCreateOptions` | `SessionCreateOptions` | session_memory | |
| `ForkOptions` | `ForkOptions` | session_memory, session_jsonl, differential | Variants cover default/at/before/tree. |
| `SessionRepo` | inherent repo methods | session_memory, session_jsonl | No shared Rust trait: the two backends have distinct metadata types. |
| `SessionErrorCode` | `SessionErrorCode` | all session tests | Exact `as_str()` literals. |
| `SessionError` | `SessionError` + `SessionErrorCause` | all session tests | Code/message plus structured file/decode/session cause. |

## Branches

- `Entry`/`LaneRecord` bodies are internally tagged and flattened; the discriminant is emitted first and storage fields last, matching TS `{...body, parentId, seq, timestamp}`.
- `OptionalValue` distinguishes an absent `data`/`details` property from an explicit JSON `null`; TS `undefined` → `Missing`, `null` → `Present(Null)`.
- `Usage`, `tokensBefore`, `durationMs`, `attempt`, and `toolIndex` use `pi_js_compat::number` so integral values print without `.0`.
- Query validation (`limit > 0`, cursor `>= 0`) lives in `state.rs` and the facade.

## Explicit gaps

- Invalid JavaScript runtime values (`undefined`, `bigint`, `Map`, cycles, sparse arrays, accessors) are not representable in the typed Rust API. `assertJsonSerializable` is therefore reduced to rejecting non-finite numbers in `Session::append_entry`/`append_record`.
- Unknown extra properties on entry/record JSON objects are ignored by typed deserialization. TS spreads them through and would re-emit them on fork. Files written by TS through the public API never contain such properties; this is a typed-boundary exclusion, not a source discrepancy.
- `SessionRepo` has no shared Rust trait because `Session<M>` is generic over metadata and the repo methods already share behavior through `SessionStorage`.
