# session-history.ts

## Storage-3 open failure (2026-10-25)

`repin_storage_history::missing_compacted_transcript_preserves_frozen_open_error` opens the same TS-created compacted transcript in both managers before frozen removes it. Both emit the three retained branch entries before reading ancestors; native now retains the original Node ENOENT open error and then exhausts the iterator. Source/native RED `/tmp/storage3-history-open-red.log` proved the former OS Display message differed. The shared Node filesystem record carries constructor/code/errno/syscall/path through SessionManagerError. Archive decoding and whole-module acceptance remain separate.

## Storage-3 archive decoder record (2026-10-25)

`corrupted_archive_history_preserves_frozen_decoder_error_and_cleanup` opens the same frozen-created compacted archive before the oracle replaces its bytes. Both yield the retained branch before failing. Text already matched; the stronger RED `/tmp/storage3-history-codec-record-red.log` proved native lost the original decoder Error/code/errno. History now uses the existing spool byte-copy boundary (visibility-only change in session-transcript-spool) and preserves the decoder record instead of flattening io::copy errors. The archive is not restored or modified by native history. The earlier `/tmp/storage3-history-codec-red.log` passed and is not RED evidence.

## Storage re-pin falsy-parent correction (2026-10-25)

`repin_storage_history::empty_parent_does_not_open_removed_transcript` uses a frozen-TS-created compacted session whose first retained entry has `parentId:""`. Both implementations open the same JSONL before the source reader removes it. Frozen `session-history.ts:75-76` returns the three retained entries without opening the removed transcript; native previously returned an ENOENT error. The native initial parent/path and subsequent parent updates now use the frozen string-truthiness guards. Actual-source RED/GREEN logs: `/tmp/pi-rust-repin-storage/history-empty-parent-{valid-red,green}.log`. The initial non-compacted probe was rejected at the manager loader by both implementations, so it is not history RED evidence. Whole-module acceptance is still pending.

Frozen `readSessionHistoryReverse` → `read_session_history_reverse`. Iterator emits manager branch newest first; lazily scans physical transcript backwards for exact parent IDs, including compacted ancestors, skips inactive and malformed records/header.64KiB chunks + largest line memory. Archived history streams to private temporary disk without modifying source/context. Cleanup on exhaustion/error/drop.

`repin_session_storage::frozen_compacted_archive_history_follows_only_active_ancestors`: TS API produces inactive branch + compaction, TS history yields compacted ancestor; branch-only Rust baseline RED missed ancestor. Large/chunked, partial-tail and early-drop evidence is recorded below and in the [integration ledger](session-storage-integration.md); whole-module proof remains todo.

`large_unicode_compacted_history_streaming_and_early_drop_cleanup`: source manager creates1.35MiB Unicode ancestor, incomplete final JSON tail, TS archive; Rust13-byte decoded chunks reproduce exact original bytes; history matches same TS and early iterator drop removes its spool in isolated TMPDIR; archive remains byte-identical/no plain sibling. Source worker-only hard90 proof already recorded separately.
