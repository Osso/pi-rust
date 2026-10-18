# Traceability: `packages/agent/src/harness/session/memory.ts`

Rust: `crates/pi-agent-core/src/harness/session/memory.rs`. Tests: `session_memory.rs`, `differential_session.rs`.

## Exports and branches

| TypeScript | Rust | Tests | Notes |
|---|---|---|---|
| `InMemorySessionStorage` | `InMemorySessionStorage` | memory, differential | `SessionState` behind a `tokio::sync::Mutex`; injectable `Clock`. |
| constructor metadata clone | `new`/`with_clock` | memory | Rust clones values on read, matching `structuredClone` semantics. |
| `fork(metadata, options)` | `fork()` | memory, differential | Replays `create_fork_mutations` into fresh state. |
| `getMetadata` | `get_metadata` | differential | |
| `getLanes`/`createLane`/`moveLane` | same | memory, differential | |
| `appendEntry` | `append_entry` | memory, differential | Parent from lane leaf, clock timestamp, unused-id check. |
| `appendRecord` | `append_record` | memory, differential | Rejects a second open operation per lane with `storage`. |
| `getEntry`/`findEntries`/`findEntriesOnBranch`/`findRecords`/`findOpenOperations`/`getLog` | same | memory, differential | |
| `getName`/`setName`/`getLabel`/`setLabel`/`getStats` | same | memory, differential | |
| `InMemorySessionRepo` | `InMemorySessionRepo` | memory, differential | Injectable clock and id generator. |
| `create`/`open`/`list`/`delete`/`fork` | same | memory, differential | Duplicate ids and missing sessions map to `already_exists`/`not_found`. |

## Branches and gaps

- TS serializes writes through promise chaining; Rust uses the state mutex, which provides the same linearization.
  `Session::view` writes to different lanes serialize through the same mutex, matching the conformance expectation.
- TS `structuredClone` detaches caller mutations; Rust clones entries and records on read.
- `SessionRepo` is not a Rust trait: each backend exposes inherent methods with its own metadata type.
