# session-history.ts

Frozen `readSessionHistoryReverse` → `read_session_history_reverse`. Iterator emits manager branch newest first; lazily scans physical transcript backwards for exact parent IDs, including compacted ancestors, skips inactive and malformed records/header.64KiB chunks + largest line memory. Archived history streams to private temporary disk without modifying source/context. Cleanup on exhaustion/error/drop.

`repin_session_storage::frozen_compacted_archive_history_follows_only_active_ancestors`: TS API produces inactive branch + compaction, TS history yields compacted ancestor; branch-only Rust baseline RED missed ancestor. Whole-module row remains todo pending large/chunked, malformed, interrupted iteration proofs.
