# session-history.ts

Frozen `readSessionHistoryReverse` → `read_session_history_reverse`. Iterator emits manager branch newest first; lazily scans physical transcript backwards for exact parent IDs, including compacted ancestors, skips inactive and malformed records/header.64KiB chunks + largest line memory. Archived history streams to private temporary disk without modifying source/context. Cleanup on exhaustion/error/drop.

`repin_session_storage::frozen_compacted_archive_history_follows_only_active_ancestors`: TS API produces inactive branch + compaction, TS history yields compacted ancestor; branch-only Rust baseline RED missed ancestor. Large/chunked, partial-tail and early-drop evidence is recorded below and in the [integration ledger](session-storage-integration.md); whole-module proof remains todo.

`large_unicode_compacted_history_streaming_and_early_drop_cleanup`: source manager creates1.35MiB Unicode ancestor, incomplete final JSON tail, TS archive; Rust13-byte decoded chunks reproduce exact original bytes; history matches same TS and early iterator drop removes its spool in isolated TMPDIR; archive remains byte-identical/no plain sibling. Source worker-only hard90 proof already recorded separately.
