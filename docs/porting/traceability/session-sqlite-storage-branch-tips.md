# Traceability: `packages/session-backends/sqlite-node/src/sqlite/storage/branch-tips.ts`

Rust: `crates/pi-session-sqlite/src/sqlite/storage/branch_tips.rs`. Tests: `branch_cache.rs`,
differential (fork tips).

| TypeScript | Rust | Tests |
|---|---|---|
| `readBranchTipIds` | `read_branch_tip_ids` | branch-cache, differential |
| `readBranchTipBranchId` | `read_branch_tip_branch_id` | branch-cache |
| `insertBranchTip` | `insert_branch_tip` | branch-cache |
| `updateBranchTip` | `update_branch_tip` | branch-cache |
| `deleteBranchTips` | `delete_branch_tips` | branch-cache, repository |
