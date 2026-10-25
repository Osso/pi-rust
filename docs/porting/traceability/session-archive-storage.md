# session-archive-storage.ts

Frozen [4f9e6f8a4](../source-lock.json); six exports audited against native `session_archive_storage.rs`2026-10-25. [Contract](../../specs/session-archive-storage.md). Whole manager/control acceptance remains separate.

| Frozen export / realistic branches | Native proof |
|---|---|
| `ARCHIVED_SESSION_SUFFIX`, `isArchivedSessionFile` | exact `.zst` suffix; source/native predicate fixtures |
| `archiveSessionFile`: already archived, missing source, existing target, read/compress/validate/publish | source/native short-circuit/error matrix; reciprocal TS/native archives preserve decoded bytes and modes |
| `restoreArchivedSessionFile`: plain pass-through, missing archive with/without surviving plain, existing target, read/decode/publish | short-circuit/error matrix; source/native directory and corruption records; reciprocal decode |
| `archivePersistedSession`: file then metadata relocation, restore on DB failure | frozen-created JSONL; exact EEXIST/mkdir record and restored original bytes after source/native failure |
| `restoreArchivedSession`: stale alias, surviving plain precedence, optional metadata relocation, missing unindexed archive, restore then relocation/rollback | existing source/native metadata/mode/rollback/plain-precedence fixture plus short-circuit matrix and exact DB failure after rearchive |
| `replaceSessionFile`: original mode, exclusive temporary, chmod, rename, unlink, catch cleanup | existing reciprocal byte/mode proofs and literal source-order audit; no alternate publication path |

## Storage-3 original error records

`suite_session_1 repin_storage_archive`4 PASS, `/tmp/storage3-archive-records-green.log`. Actual frozen operations compare public Error values, constructor identity and unchanged/rolled-back files. RED `/tmp/storage3-archive-records-red.log` proved directory reads used OS Display text and DB failures lost original filesystem properties; `/tmp/storage3-archive-codec-red.log` proved corrupted frames lost code/errno. Native reads reuse shared Node read and decoder-error authorities. Metadata wrappers reuse existing manager `control_db_error` (visibility-only pub(super)), without new carriers, dependencies or fallback.

Current source/native reciprocal archive/restore and mode/rollback/plain-precedence tests both PASS atd0f4c85ae (`/tmp/storage3-archive-{rust_archive_restores_with_frozen_ts_and_exact_bytes,archive_restore_modes_metadata_rollback_and_surviving_plain_precedence}.log`). Earlier broader [storage integration](session-storage-integration.md) remains bounded by its own ledger. Strict protected fixture typecheck and Biome pass. Warning-mode scoped storage Clippy passes with zero findings in archive code; current Cargo check passes. The new archive test reuses the existing suite history oracle helper instead of adding a duplicate module warning. Manual changed-line readability audit found no structural issue; metrics executable unavailable. Exact commands/revisions: `/tmp/storage3-proof-ledger.md`.

Gap: entropy, codec self-validation faults, exceptional publication/unlink/cleanup failures and runtime races follow current OS/runtime exclusions; no fault injection expansion or new carrier was added.
