# Traceability: `packages/agent/src/harness/session/session.ts`

Rust: `crates/pi-agent-core/src/harness/session/facade.rs` (module named `facade` instead of `session` to satisfy
`clippy::module_inception`; the type remains `Session`). Tests: `session_memory.rs`, `session_jsonl.rs`,
`differential_session.rs`.

## Exports and branches

| TypeScript | Rust | Tests | Notes |
|---|---|---|---|
| `assertJsonSerializable` | `validate_entry_payload` / `validate_record_payload` | memory | Typed values cannot be undefined/bigint/Map/cyclic; non-finite usage/attempt numbers raise `invalid_payload`. |
| `Session` constructor `idGenerator` | `Session::new`, `Session::with_id_generator` | memory | Default UUIDv7 generator. |
| `view(lane)` | `Session::view` → `SessionView` | memory | `main` behaves like the session. |
| `getMetadata` | `get_metadata` | differential | |
| `getLeafId` | `get_leaf_id` | memory, differential | Missing lane → `invalid_lane`. |
| `getEntry`/`getStats`/`getName`/`setName`/`getLabel`/`setLabel` | same | memory, differential | |
| `findEntries`/`findEntry` | same | memory, differential | `findEntry` caps the storage query at 1 after validating the caller limit. |
| `findEntriesOnBranch`/`findEntryOnBranch` | same | memory, differential | Defaults start to the view's lane leaf; `null` leaf returns empty. |
| `appendMessage` | `append_message` | memory, differential | ID from the shared generator. |
| `appendCompaction` | `append_compaction` | memory, jsonl | Options record maps to `CompactionAppendOptions`. |
| `appendCustomEntry` | `append_custom_entry` | memory, jsonl | `None` omits `data`; `Some(Value::Null)` keeps explicit null. |
| `getLanes`/`createLane`/`moveLane` | same | memory, differential | |
| `appendEntry` | `append_entry` | memory, jsonl, differential | Payload validation before storage. |
| `appendRecord` | `append_record` | memory, jsonl, differential | Payload validation before storage. |
| `findRecords` | `find_records` | memory, differential | `operationKind` requires `type: operation_started`. |
| `findOpenOperations` | `find_open_operations` | memory, differential | Validates limit. |
| `getLog` | `get_log` | memory, differential | Validates limit/cursor. |
| `SessionTree` view subset | `SessionView` | memory | View methods delegate to lane-aware private helpers. |

## Branches and gaps

- TS `appendRecord` returns `TNewRecord & Pick<RecordBase, "seq" | "timestamp">`; Rust returns the stored `LaneRecord`.
- TS overloads on `findRecords` collapse to one Rust method returning `Vec<LaneRecord>`.
- `assertJsonSerializable` rejections for JS-only values are an explicit typed-boundary exclusion (see
  `session-types.md`).
