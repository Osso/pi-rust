# Traceability: `packages/session-backends/sqlite-node/src/sqlite/branch-cache.ts`

Rust: `crates/pi-session-sqlite/src/sqlite/branch_cache.rs`. Tests: `branch_cache.rs`,
`branch_query.rs`, differential.

| TypeScript | Rust | Tests | Notes |
|---|---|---|---|
| `deleteBranchCache` | `delete_branch_cache` | branch-cache | Tips then entries. |
| `rebuildBranchCache` | `rebuild_branch_cache` | branch-cache | Leaf query (`NOT EXISTS child`) ordered by seq; rebuilds every tip. |
| `buildCachedBranch` | `build_cached_branch` | branch-cache, differential | `SAVEPOINT build_branch_cache`; on failure roll back/release and rethrow `SessionError` or wrap as `storage` with `Failed to build SQLite branch cache at entry {leaf}`. |
| `extendBranch` | `extend_branch` | branch-cache | Inserts the entry and moves the tip; `Branch tip {id} changed during append`. |
| `appendEntryToBranchCache` | `append_entry_to_branch_cache` | branch-cache, conformance | Root → new branch; tip parent → extend; otherwise copy the containing branch through the parent seq. Missing parent → `Branch cache has no branch containing parent entry {id}`. |

Branch ids are fresh UUIDv7 values in both implementations and are not part of any public
response, so they are excluded from differential comparison.
