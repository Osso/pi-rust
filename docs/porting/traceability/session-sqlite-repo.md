# Traceability: `packages/session-backends/sqlite-node/src/sqlite/repo.ts`

Rust: `crates/pi-session-sqlite/src/sqlite/repo/{mod,storage,decode,fork}.rs`. Tests: `conformance.rs`, `repository.rs`,
`branch_cache.rs`, `branch_query.rs`, `log_query.rs`, `writer_leases.rs`,
`differential_session_sqlite.rs`, `cross_implementation.rs`.

## Exports

| TypeScript | Rust | Tests | Notes |
|---|---|---|---|
| `SqliteSessionRepository` | same | all | Concrete type; the source `SessionRepo` interface has no Rust trait. |
| `SqliteSessionRepositoryOptions` | same (`env: Arc<dyn FileSystem>`) | all | |
| `SqliteWriterLeaseOptions` | same | writer-leases | Defaults 30s/10s. |
| `resolveWriterLeaseOptions` | `resolve_writer_lease_options` | writer-leases | Returns `SqliteWriterLeaseOptionsError` with the exact `RangeError` messages. |
| `activeWriterError`/`lostWriterError` | same | writer-leases | Exact messages. |
| `SerialOperationQueue` | repo `operations` mutex + storage `write_lock` | writer-leases | Serializes repository operations and per-session writes. |
| `resultOrThrow` | `result_or_throw` | repository | Maps `FileErrorCode::NotFound` → `not_found`, else `storage`; message `{context}: {error}`. |
| `getParentPath` | `get_parent_path` | cross (db creation) | Trailing separators, `/` and `\`, root and no-separator cases. |
| `configureSqliteDatabase` | `configure_sqlite_database` | migrations, cross | `journal_mode=WAL`, `synchronous=FULL`, `busy_timeout=5000`. |
| `entryRowFromCached` | `entry_row_from_cached` | branch-query | |
| `decodeEntry` | `decode_entry` (`repo/decode.rs`) | repository, branch-query, log-query | Per-type validation with `Invalid SQLite session entry {id}: failed to decode entry {id}`. |
| `readObjectPayload` | inline object check | repository | Non-object payload → `invalid_entry`. |
| `recordRunId`/`recordOpKind` | same | conformance | `operation_started` uses the record id and intent kind. |
| `decodeRecord` | `decode_record` | repository | Payload + stored `seq`/`timestamp`; failure → `storage` `... failed to decode payload`. |
| `validateCachedBranchRows` | `validate_cached_branch_rows` | branch-query | Root inclusion rule and parent-chain check with the source messages. |
| `matchesEntryQuery` | `matches_entry_query` | conformance | |
| `assertUnusedId` | `assert_unused_id` | conformance | |
| `requireSessionRow` | `require_session_row` | repository | |
| `SqliteSessionStorage` | `SqliteSessionStorage` (`repo/storage.rs`) | all | Implements `SessionStorage<SqliteSessionMetadata>`. |
| `claimWriterLease`/`claimStorage`/`sessionFromLease` | `claim_writer_lease`/`claim_storage`/`build_storage` | writer-leases | |
| `create` | `create` | conformance, repository, differential | Insert + sequence + stats + initial lane + lease in one transaction. |
| `open` | `open` | writer-leases, conformance | Reuses the active storage (no second lease) or claims one. |
| `repairBranchCache` | `repair_branch_cache` | branch-cache | Releases storages, claims, rebuilds, releases the lease. |
| `list` | `list` | conformance, repository, differential | Returns `[]` before the database file exists; never takes leases. |
| `delete` | `delete` | conformance, repository, search | Deletes cache/facts/lanes/records/entries/lease/stats/sequence/session in one transaction; idempotent. |
| `fork` | `fork` | conformance, branch-cache, differential | Branch/tree scopes, labels/name copy, seq allocation, cache rebuild, writer error wrapping `Failed to fork SQLite session {id}`. |
| `close` | `close` | repository, writer-leases | Releases active storages, closes the connection once; idempotent. |
| `[Symbol.asyncDispose]` | `close` | repository | Explicit call. |

## Write/read branches

| Branch | Tests |
|---|---|
| session write rejected after release (`SQLite session {id} is closed`) | repository |
| lease renewal before every write; loss latches `writer lease was lost` | writer-leases |
| heartbeat renewal on idle sessions | writer-leases |
| heartbeat aborted on release/loss | writer-leases |
| transaction rollback leaves no entry, lane leaf, or stats change | repository |
| entry append updates parent, lane leaf, branch cache, message count, sequence | conformance |
| record append enforces lane existence, open-operation uniqueness, usage accounting | conformance |
| `findEntries` custom-type filter and post-limit slice | conformance, differential |
| branch reads validate only the returned window | branch-query |
| log rows are decoded only after the combined limit | log-query |
| read-only paths keep working after release (source behavior) | branch-cache |

## Gaps and boundary deviations

- `SessionStorage`/`SessionSearch` only carry `SessionError`; raw SQLite failures (e.g. trigger
  aborts) surface with code `storage` and the SQLite message, while TypeScript throws an
  uncoded `Error`. Messages match.
- The `SqliteDatabaseFactory` injection points are gone; setup failure, connection retention, and
  close-once behavior are proven with real files and triggers.
- `SessionRepo` has no Rust trait (the JSONL backend likewise exposes a concrete repository).
- Fork/repository methods return `SessionError` (no raw error passthrough).
- Unknown entry `type` rows: TypeScript's `decodeEntry` falls off the switch and returns
  `undefined`; Rust reports `invalid_entry`. Recorded as a source bug discrepancy, not preserved.
- Payload shape checks are stricter in Rust where TypeScript only casts: `message: []` passes the
  source's `typeof === "object"` guard, and `retainedTail` elements are not validated, while serde
  rejects both. Both produce the same `invalid_entry` message for genuinely malformed payloads;
  the difference only shows for values TypeScript would accept and then mis-typed.
