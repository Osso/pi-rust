# Traceability: `packages/session-backends/sqlite-node/src/sqlite/storage/branch-entries.ts`

Rust: `crates/pi-session-sqlite/src/sqlite/storage/branch_entries.rs`. Tests: `branch_cache.rs`,
`branch_query.rs`, differential.

| TypeScript | Rust | Tests | Notes |
|---|---|---|---|
| `CachedBranch` | `CachedBranch { branch_id: String, leaf_seq }` | branch-cache | Branch ids are UUIDv7 strings in both implementations. |
| `CachedBranchEntryRow` | same | branch-query | |
| `readCachedBranch` | `read_cached_branch` | branch-cache, conformance | `ORDER BY branch_id LIMIT 1`. |
| `queryCachedBranchRows` | `query_cached_branch_rows` | branch-query, conformance | Same stop-boundary subquery with `MIN`/`MAX` and `COALESCE`, cursor direction, type/custom filters, limit. |
| `deleteBranchEntries` | `delete_branch_entries` | branch-cache | |
| `insertBranchEntry` | `insert_branch_entry` | branch-cache | |
| `insertBranchEntriesForPath` | `insert_branch_entries_for_path` | branch-cache | Walks parents, detects cycles (`Entry parent cycle at {id}`), inserts root→leaf. |
| `customTypeFromPayload` | `custom_type_from_payload` | branch-cache | Errors map to `invalid_entry` with the source message. |
| `readBranchContainingEntry` | `read_branch_containing_entry` | branch-cache | |
| `copyBranchEntriesThroughSeq` | `copy_branch_entries_through_seq` | branch-cache | |
