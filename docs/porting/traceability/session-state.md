# Traceability: `packages/agent/src/harness/session/state.ts`

Rust: `crates/pi-agent-core/src/harness/session/state.rs`. Tests: `session_memory.rs`, `session_jsonl.rs`,
`differential_session.rs`.

## Exports and branches

| TypeScript | Rust | Tests | Notes |
|---|---|---|---|
| `SessionMutation` | `SessionMutation` | codec, memory, jsonl | Serialized with `kind` tag; entry/record bodies flattened. |
| `SessionState.nextSequence` | `next_sequence()` | memory, jsonl | Monotonic `i64`. |
| `getLanes` | `get_lanes()` | memory, differential | Insertion order, `main` first. |
| `requireLane` | `require_lane()` | memory | `invalid_lane`, `Lane not found: {lane}`. |
| `validateNewLane` | `validate_new_lane()` | memory | `already_exists`. |
| `validateTarget` | `validate_target()` | memory, jsonl | `not_found`. |
| `validateUnusedId` | `validate_unused_id()` | memory, jsonl | `already_exists`. |
| `applyMutation` seq check | `apply_mutation()` | jsonl replay | `Invalid session mutation: has non-consecutive seq {seq}`. |
| entry mutation validation | `apply_entry()` | jsonl replay | duplicate id, missing lane, lane-leaf chaining, missing parent. |
| record mutation validation | `apply_record()` | jsonl replay | missing lane, duplicate id; open-operation map maintenance; usage stats. |
| lane mutation | `apply_mutation` lane arm | memory, jsonl | missing target check, lane pointer replacement, log item. |
| fact mutation | `apply_fact()` | memory, jsonl | label target check, latest-value name/labels, log item. |
| `findEntries` | `find_entries()` | memory, differential | order, type, customType, cursor, limit. |
| `findEntriesOnBranch` | `find_entries_on_branch()` | memory, differential | oldestFirst walks full path then reverses and applies bounds after matching; newestFirst stops during walk. |
| `findRecords` | `find_records()` | memory, differential | lane/type/runId/operationKind/afterSeq. |
| `findOpenOperations` | `find_open_operations()` | memory, differential | newest-first map insertion order, optional limit. |
| `getLog` | `get_log()` | memory, differential | `afterSeq` exclusive, limit. |
| `getName`/`getLabel`/`getStats` | same | memory, differential | |
| `createForkMutations` | `create_fork_mutations()` | memory, jsonl, differential | tree copies entries then lanes then name then labels; branch requires a message target and defaults position by `entryId` presence. |
| `walkToRoot` | `walk_to_root()` | memory | cycle detection, missing parent, bound stop. |

## Branches and discrepancies

- The TS `invalid` callback parameter on `applyMutation` has no caller that overrides it; Rust always raises the same `invalid_entry` error.
- `findOpenOperations` returns `OperationStartedRecord[]` in TS; Rust returns `LaneRecord` values whose body is known to be `OperationStarted`, matching the runtime shape.
- Label maps and entry indexes use `HashMap`; only lane order is observable and is preserved with a `Vec`.
