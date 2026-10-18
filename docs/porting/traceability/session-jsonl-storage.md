# Traceability: `packages/agent/src/harness/session/jsonl/storage.ts`

Rust: `crates/pi-agent-core/src/harness/session/jsonl/storage.rs`. Tests: `session_jsonl.rs`,
`differential_session.rs`.

## Exports and branches

| TypeScript | Rust | Tests | Notes |
|---|---|---|---|
| `publishFileAtomically` | `publish_file_atomically` / inline fork publish | jsonl torn-tail/fork failure | Writes `{destination}.tmp`, renames over destination, removes temp on failure. |
| `JsonlSessionStorage.create` | `JsonlSessionStorage::create` | jsonl, differential | Header write then `fileInfo` → metadata. |
| `JsonlSessionStorage.load` | `JsonlSessionStorage::load` | jsonl, differential | Splits physical lines, requires header, replays mutations. |
| missing header | `load` | jsonl | `invalid_entry` line 1 `is missing a header`. |
| torn final syntax error | `load` | jsonl repair tests | Publishes valid prefix atomically and returns storage. |
| invalid interior/final mutation | `load` | jsonl replay tests | `invalid_file`; file unchanged. |
| replay `invalid_entry` | `load` | jsonl replay tests | Wrapped as `Invalid JSONL v4 session …`. |
| unterminated final line | `load` | jsonl repair tests | Appends `\n`; failure maps to `storage`. |
| `fork` | `JsonlSessionStorage::fork` | jsonl, differential | Copies fork mutations to a temp file, renames, reloads; no partial publication. |
| `drain` | `drain` | – | Mutex serialization has no promise tail; no-op. |
| `getMetadata`/`getLanes`/`createLane`/`moveLane`/`appendEntry`/`appendRecord` | same | jsonl, differential | Mutations are appended before state application; append failures leave state and queue usable. |
| read methods | same | jsonl, differential | State mutex; values cloned on return. |
| `getName`/`setName`/`getLabel`/`setLabel`/`getStats` | same | jsonl, differential | Facts append before apply. |
| `enqueue` | state mutex | jsonl append-failure test | Serializes writes across lanes. |
| `appendMutation` | `append_mutation` | jsonl | `Failed to append session {path}` mapping. |

## Branches and gaps

- TS `structuredClone` semantics are achieved by cloning entries/records on read.
- Read methods in TS are not enqueued behind the write tail; Rust reads take the same state mutex, which can wait
  for an in-flight append. Observable results are identical; only concurrency timing differs.
- The TS constructor takes metadata directly; Rust adds `with_clock` so timestamps are injectable for differential
  runs.
